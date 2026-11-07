use std::{cell::RefCell, rc::Rc};
use zui_api::{event::*, layout::*, widget::*};
use zui_core::{Tree, TreeError};
use zui_paint::{Command, Painter};
use zui_types::{Affine, Color, Point, Rect, Size};

#[derive(Default)]
struct Probe {
    viewport: ChildViewport,
    input: bool,
    foreground: Option<Rect>,
    layout_on_focus: bool,
    reveal: bool,
    reveals: Rc<RefCell<Vec<Rect>>>,
    log: Rc<RefCell<Vec<EventContext>>>,
}
impl Widget for Probe {
    fn child_viewport(&self, _: PaintContext) -> ChildViewport {
        self.viewport
    }
    fn reveal_child(&mut self, _: PaintContext, area: Rect) -> bool {
        if !self.reveal {
            return false;
        }
        self.reveals.borrow_mut().push(area);
        self.viewport.translation.y -= area.origin.y;
        area.origin.y != 0.0
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        painter.fill_rect(
            Rect::new(0.0, 0.0, context.size.width, context.size.height),
            Color::WHITE,
        );
    }
    fn hit_test(&self, point: Point, size: Size) -> bool {
        self.input && Rect::new(0.0, 0.0, size.width, size.height).contains(point)
    }
    fn paint_foreground(&self, _: PaintContext, painter: &mut Painter<'_>) {
        if let Some(rect) = self.foreground {
            painter.fill_rect(rect, Color::BLACK);
        }
    }
    fn hit_test_foreground(&self, point: Point, _: PaintContext) -> bool {
        self.foreground.is_some_and(|rect| rect.contains(point))
    }
    fn focusable(&self) -> bool {
        self.input
    }
    fn text_input(&self, _: PaintContext) -> Option<TextInputState> {
        self.input.then_some(TextInputState {
            generation: 0,
            cursor_area: Rect::new(5.0, 5.0, 1.0, 12.0),
        })
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        self.log.borrow_mut().push(context);
        if self.layout_on_focus && matches!(event, WidgetEvent::FocusChanged(true)) {
            return EventResponse {
                invalidation: Invalidation::Layout,
                ..Default::default()
            };
        }
        if self.input
            && context.phase == EventPhase::Target
            && matches!(
                event,
                WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
            )
        {
            EventResponse {
                focus: FocusRequest::Focus,
                capture: CaptureRequest::Capture,
                ..Default::default()
            }
        } else {
            EventResponse::default()
        }
    }
}
struct FixedLayout(Vec<(NodeId, Rect)>);
impl LayoutEngine for FixedLayout {
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        _: NodeId,
        _: Size,
    ) -> Result<(), LayoutError> {
        for &(id, bounds) in &self.0 {
            tree.set_layout(id, LayoutBox::new(bounds));
        }
        Ok(())
    }
}
fn setup() -> (Tree, NodeId, Rc<RefCell<Vec<EventContext>>>) {
    let mut tree = Tree::new(
        Probe {
            viewport: ChildViewport {
                translation: Point::new(0.0, -30.0),
                clip: Some(Rect::new(10.0, 10.0, 60.0, 40.0)),
            },
            ..Default::default()
        },
        LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        },
        Default::default(),
    )
    .unwrap();
    let log = Rc::default();
    let child = tree
        .insert(
            tree.root(),
            0,
            Probe {
                input: true,
                log: Rc::clone(&log),
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    tree.layout(
        &mut FixedLayout(vec![
            (tree.root(), Rect::new(10.0, 20.0, 100.0, 80.0)),
            (child, Rect::new(10.0, 40.0, 40.0, 80.0)),
        ]),
        Size::new(200.0, 200.0),
    )
    .unwrap();
    (tree, child, log)
}

#[test]
fn child_presentation_moves_paint_hits_capture_and_ime_without_moving_layout() {
    let (mut tree, child, log) = setup();
    let original = tree.layout_box(child).unwrap();
    let scene = tree.scene().unwrap();
    let fills: Vec<_> = scene
        .commands()
        .iter()
        .filter_map(|command| match command {
            Command::Fill { transform, .. } => Some(*transform),
            _ => None,
        })
        .collect();
    assert_eq!(
        fills,
        [Affine::translate(10.0, 20.0), Affine::translate(20.0, 30.0)]
    );
    assert!(
        matches!(scene.commands()[1], Command::PushClip { transform, .. } if transform == Affine::translate(10.0, 20.0))
    );
    assert_eq!(
        tree.hit_test(Point::new(25.0, 35.0))
            .unwrap()
            .unwrap()
            .local,
        Point::new(5.0, 5.0)
    );
    assert!(tree.hit_test(Point::new(25.0, 75.0)).unwrap().is_none());
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(25.0, 35.0)),
    })
    .unwrap();
    assert_eq!(tree.focused(), Some(child));
    assert_eq!(tree.pointer_capture(), Some(child));
    assert_eq!(
        tree.text_input().unwrap().unwrap().cursor_area,
        Rect::new(25.0, 35.0, 1.0, 12.0)
    );

    tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |probe| {
        probe.viewport.clip = Some(Rect::new(15.0, 10.0, 0.0, 40.0));
    })
    .unwrap();
    assert!(tree.text_input().unwrap().is_none());
    assert!(tree.hit_test(Point::new(25.0, 35.0)).unwrap().is_none());
    tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |probe| {
        probe.viewport.clip = Some(Rect::new(10.0, 10.0, 60.0, 40.0));
    })
    .unwrap();

    // A presentation-only update needs neither measurement nor layout publication.
    tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |probe| {
        probe.viewport.translation.y = -40.0;
    })
    .unwrap();
    assert_eq!(tree.layout_box(child).unwrap(), original);
    assert_eq!(
        tree.text_input().unwrap().unwrap().cursor_area,
        Rect::new(25.0, 30.0, 1.0, 7.0)
    );
    log.borrow_mut().clear();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(150.0, 150.0)))
        .unwrap();
    let event = *log
        .borrow()
        .iter()
        .find(|e| e.phase == EventPhase::Target)
        .unwrap();
    assert_eq!(event.local_position, Some(Point::new(130.0, 130.0)));
    tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |probe| {
        probe.viewport.translation.y = -80.0;
    })
    .unwrap();
    assert!(tree.text_input().unwrap().is_none());
}

#[test]
fn invalid_presentation_is_rejected_before_events_or_paint_and_can_recover() {
    let (mut tree, _, log) = setup();
    for viewport in [
        ChildViewport {
            translation: Point::new(f64::NAN, 0.0),
            clip: None,
        },
        ChildViewport {
            translation: Point::ZERO,
            clip: Some(Rect::new(0.0, 0.0, -1.0, 4.0)),
        },
        ChildViewport {
            translation: Point::new(f64::MAX, 0.0),
            clip: Some(Rect::new(f64::MAX, 0.0, f64::MAX, 4.0)),
        },
    ] {
        tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |p| p.viewport = viewport)
            .unwrap();
        assert!(matches!(
            tree.scene(),
            Err(TreeError::InvalidChildViewport(_))
        ));
        assert!(matches!(
            tree.hit_test(Point::ZERO),
            Err(TreeError::InvalidChildViewport(_))
        ));
        assert!(matches!(
            tree.dispatch_input(InputEvent::PointerMoved(Point::ZERO)),
            Err(TreeError::InvalidChildViewport(_))
        ));
        assert!(log.borrow().is_empty());
    }
    tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |p| {
        p.viewport = ChildViewport::default()
    })
    .unwrap();
    assert!(tree.scene().is_ok());
    assert!(tree.hit_test(Point::new(25.0, 65.0)).unwrap().is_some());
}

#[test]
fn nested_viewports_compose_and_child_clips_do_not_clip_the_owner() {
    let (mut tree, branch, _) = setup();
    tree.set_style(
        branch,
        LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        },
    )
    .unwrap();
    tree.update_widget::<Probe>(branch, Invalidation::Paint, |p| {
        p.viewport = ChildViewport {
            translation: Point::new(-5.0, -10.0),
            clip: Some(Rect::new(0.0, 10.0, 30.0, 20.0)),
        };
    })
    .unwrap();
    let leaf = tree
        .insert(
            branch,
            0,
            Probe {
                input: true,
                ..Default::default()
            },
            LayoutStyle::default(),
        )
        .unwrap();
    tree.layout(
        &mut FixedLayout(vec![
            (tree.root(), Rect::new(10.0, 20.0, 100.0, 80.0)),
            (branch, Rect::new(10.0, 40.0, 40.0, 80.0)),
            (leaf, Rect::new(10.0, 20.0, 40.0, 40.0)),
        ]),
        Size::new(200.0, 200.0),
    )
    .unwrap();
    let hit = tree.hit_test(Point::new(30.0, 45.0)).unwrap().unwrap();
    assert_eq!(hit.id, leaf);
    assert_eq!(hit.local, Point::new(5.0, 5.0));
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(30.0, 45.0)),
    })
    .unwrap();
    assert_eq!(
        tree.text_input().unwrap().unwrap().cursor_area,
        Rect::new(30.0, 45.0, 1.0, 12.0)
    );
    // Outside the branch's child clip but inside the branch itself.
    assert_eq!(
        tree.hit_test(Point::new(30.0, 65.0)).unwrap().unwrap().id,
        branch
    );
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: false,
        position: Some(Point::new(30.0, 45.0)),
    })
    .unwrap();
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(30.0, 65.0)),
    })
    .unwrap();
    assert_eq!(
        tree.text_input().unwrap().unwrap().cursor_area,
        Rect::new(25.0, 35.0, 1.0, 12.0)
    );
    let scene = tree.scene().unwrap();
    let transforms: Vec<_> = scene
        .commands()
        .iter()
        .filter_map(|cmd| match cmd {
            Command::Fill { transform, .. } => Some(*transform),
            _ => None,
        })
        .collect();
    assert_eq!(
        transforms,
        [
            Affine::translate(10.0, 20.0),
            Affine::translate(20.0, 30.0),
            Affine::translate(25.0, 40.0)
        ]
    );
    // Existing node clipping also remains in effect, independently of child clips.
    tree.set_clip(tree.root(), true).unwrap();
    tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |p| {
        p.viewport.clip = None;
        p.viewport.translation = Point::new(150.0, -30.0);
    })
    .unwrap();
    assert!(tree.hit_test(Point::new(175.0, 45.0)).unwrap().is_none());
    assert!(tree.text_input().unwrap().is_none());
}

#[test]
fn foreground_paints_and_hits_above_children_outside_its_own_child_clip() {
    let (mut tree, _, _) = setup();
    let root = tree.root();
    tree.update_widget::<Probe>(root, Invalidation::Paint, |p| {
        // Extends beyond the child clip and beyond the parent's border box.
        p.foreground = Some(Rect::new(10.0, 10.0, 110.0, 70.0));
    })
    .unwrap();
    for point in [Point::new(25.0, 35.0), Point::new(115.0, 95.0)] {
        assert_eq!(tree.hit_test(point).unwrap().unwrap().id, root);
    }
    let scene = tree.scene().unwrap();
    assert!(
        matches!(scene.commands().last().unwrap(), Command::Fill { color: Color::BLACK, transform, .. } if *transform == Affine::translate(10.0, 20.0))
    );
    // Child clip is popped before foreground, while node clipping surrounds both.
    assert!(matches!(
        scene.commands()[scene.commands().len() - 2],
        Command::PopClip
    ));
    tree.set_clip(root, true).unwrap();
    assert!(tree.hit_test(Point::new(115.0, 95.0)).unwrap().is_none());
    let scene = tree.scene().unwrap();
    assert!(matches!(scene.commands().last().unwrap(), Command::PopClip));
    assert!(matches!(
        scene.commands()[scene.commands().len() - 2],
        Command::Fill {
            color: Color::BLACK,
            ..
        }
    ));
}

#[test]
fn focus_reveal_waits_for_layout_and_invalid_explicit_requests_do_not_call_widgets() {
    let (mut tree, child, _) = setup();
    let root = tree.root();
    let calls = Rc::default();
    tree.update_widget::<Probe>(root, Invalidation::None, |p| {
        p.reveal = true;
        p.reveals = Rc::clone(&calls);
    })
    .unwrap();
    tree.update_widget::<Probe>(child, Invalidation::None, |p| p.layout_on_focus = true)
        .unwrap();
    let response = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            position: Some(Point::new(25.0, 35.0)),
        })
        .unwrap();
    assert_eq!(response.invalidation, Invalidation::Layout);
    assert!(calls.borrow().is_empty());
    assert_eq!(
        tree.reveal_rect(child, Rect::new(0.0, 0.0, 10.0, 10.0)),
        Err(TreeError::LayoutRequired)
    );
    let mut engine = FixedLayout(vec![
        (root, Rect::new(10.0, 20.0, 100.0, 80.0)),
        (child, Rect::new(10.0, 40.0, 40.0, 80.0)),
    ]);
    tree.layout(&mut engine, Size::new(200.0, 200.0)).unwrap();
    assert_eq!(&*calls.borrow(), &[Rect::new(10.0, 10.0, 40.0, 80.0)]);
    assert!(!tree.layout(&mut engine, Size::new(200.0, 200.0)).unwrap());
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        tree.reveal_rect(child, Rect::new(f64::NAN, 0.0, 10.0, 10.0)),
        Err(TreeError::InvalidRevealArea(child))
    );
    assert_eq!(calls.borrow().len(), 1);
}

#[test]
fn presented_bounds_include_parent_origin_and_scroll_and_require_layout() {
    let (mut tree, child, _) = setup();
    assert_eq!(
        tree.presented_bounds(child).unwrap(),
        Rect::new(20.0, 30.0, 40.0, 80.0)
    );
    tree.update_widget::<Probe>(tree.root(), Invalidation::Paint, |p| {
        p.viewport.translation.y = -40.0
    })
    .unwrap();
    assert_eq!(
        tree.presented_bounds(child).unwrap(),
        Rect::new(20.0, 20.0, 40.0, 80.0)
    );
    assert!(matches!(
        tree.presented_bounds(NodeId(u64::MAX)),
        Err(TreeError::MissingNode(_))
    ));
    tree.invalidate(Invalidation::Layout);
    assert_eq!(tree.presented_bounds(child), Err(TreeError::LayoutRequired));
}
