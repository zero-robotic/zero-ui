use std::{cell::RefCell, rc::Rc};
use zui_api::{
    event::*,
    layout::{LayoutBox, LayoutEngine, LayoutError, LayoutKind, LayoutStyle, LayoutTree},
    view::{View, ViewSpec},
    widget::{Invalidation, Lifecycle, PaintContext, Widget},
};
use zui_core::{Tree, TreeError, TreeLimits};
use zui_paint::Painter;
use zui_types::{NodeId, Point, Rect, Size};

#[derive(Clone, Debug)]
enum Record {
    Event(EventContext, WidgetEvent),
    Unmount(NodeId),
}
type Log = Rc<RefCell<Vec<Record>>>;
#[derive(Clone)]
struct Probe {
    log: Log,
    focusable: bool,
    capture: bool,
    stop: bool,
    consume_tab: bool,
}
impl Widget for Probe {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn focusable(&self) -> bool {
        self.focusable
    }
    fn lifecycle(&mut self, id: NodeId, event: Lifecycle) {
        if event == Lifecycle::Unmounted {
            self.log.borrow_mut().push(Record::Unmount(id));
        }
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        self.log
            .borrow_mut()
            .push(Record::Event(context, event.clone()));
        let press = matches!(
            event,
            WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
        );
        EventResponse {
            actions: Vec::new(),
            handled: self.consume_tab
                && matches!(event, WidgetEvent::Input(InputEvent::Key(key)) if key.key == Key::Tab),
            stop_propagation: self.stop && context.phase == EventPhase::Capture,
            invalidation: if context.phase == EventPhase::Direct {
                Invalidation::Paint
            } else {
                Invalidation::None
            },
            focus: if press && context.phase == EventPhase::Target && self.focusable {
                FocusRequest::Focus
            } else {
                FocusRequest::None
            },
            capture: if press && context.phase == EventPhase::Target && self.capture {
                CaptureRequest::Capture
            } else {
                CaptureRequest::None
            },
        }
    }
}
impl ViewSpec for Probe {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        *widget = self.clone();
        Invalidation::None
    }
}
fn description(log: &Log, leaf: bool) -> View {
    let probe = |focusable, capture| Probe {
        log: log.clone(),
        focusable,
        capture,
        stop: false,
        consume_tab: false,
    };
    let mut children = Vec::new();
    if leaf {
        children.push(View::new(probe(true, true)).keyed("leaf"));
    }
    children.push(View::new(probe(true, false)).keyed("sibling"));
    View::new(probe(false, false))
        .styled(LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        })
        .with_children([View::new(probe(false, false))
            .keyed("branch")
            .styled(LayoutStyle {
                kind: LayoutKind::Column,
                ..Default::default()
            })
            .with_children(children)])
}
struct Layout;
impl LayoutEngine for Layout {
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        root: NodeId,
        _: Size,
    ) -> Result<(), LayoutError> {
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            pending.extend(tree.node(id).unwrap().children.iter().copied());
            let bounds = match id.0 {
                1 => Rect::new(10.0, 20.0, 200.0, 100.0),
                2 => Rect::new(5.0, 7.0, 100.0, 80.0),
                3 => Rect::new(3.0, 4.0, 20.0, 20.0),
                _ => Rect::new(30.0, 4.0, 20.0, 20.0),
            };
            tree.set_layout(id, LayoutBox::new(bounds));
        }
        Ok(())
    }
}
fn setup() -> (Tree, Log) {
    let log = Log::default();
    let mut tree = Tree::from_view(description(&log, true), TreeLimits::default()).unwrap();
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    tree.scene().unwrap();
    (tree, log)
}
fn button(button: PointerButton, pressed: bool, point: Point) -> InputEvent {
    InputEvent::PointerButton {
        button,
        pressed,
        position: Some(point),
    }
}
fn tab(shift: bool) -> InputEvent {
    InputEvent::Key(KeyInput {
        key: Key::Tab,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
    })
}

#[test]
fn focus_visibility_tracks_input_without_transferring_or_cancelling_focus() {
    let (mut tree, log) = setup();
    let visibility = |log: &Log| {
        log.borrow()
            .iter()
            .filter_map(|record| match record {
                Record::Event(context, WidgetEvent::FocusVisibleChanged(visible)) => {
                    Some((context.id, *visible))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let point = Point::new(20.0, 35.0);
    tree.dispatch_input(button(PointerButton::Primary, true, point))
        .unwrap();
    let focused = tree.focused().unwrap();
    assert_eq!(visibility(&log), vec![(focused, false)]);
    tree.dispatch_input(button(PointerButton::Primary, false, point))
        .unwrap();
    log.borrow_mut().clear();
    tree.dispatch_input(InputEvent::Key(KeyInput {
        key: Key::ArrowRight,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    }))
    .unwrap();
    assert_eq!(tree.focused(), Some(focused));
    assert_eq!(visibility(&log), vec![(focused, true)]);
    assert!(
        !log.borrow()
            .iter()
            .any(|record| matches!(record, Record::Event(_, WidgetEvent::FocusChanged(_))))
    );
    log.borrow_mut().clear();
    tree.dispatch_input(InputEvent::PointerMoved(point))
        .unwrap();
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Lines { x: 0.0, y: 1.0 }))
        .unwrap();
    tree.dispatch_input(InputEvent::ModifiersChanged(Modifiers {
        shift: true,
        ..Default::default()
    }))
    .unwrap();
    assert!(visibility(&log).is_empty());
    tree.dispatch_input(button(PointerButton::Primary, true, point))
        .unwrap();
    assert_eq!(tree.focused(), Some(focused));
    assert_eq!(tree.pointer_capture(), Some(focused));
    assert_eq!(visibility(&log), vec![(focused, false)]);
    tree.dispatch_input(button(PointerButton::Primary, false, point))
        .unwrap();
    log.borrow_mut().clear();
    tree.dispatch_input(InputEvent::Key(KeyInput {
        key: Key::Tab,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    }))
    .unwrap();
    assert!(visibility(&log).is_empty());
    tree.dispatch_input(tab(false)).unwrap();
    let next = tree.focused().unwrap();
    assert_ne!(next, focused);
    assert_eq!(visibility(&log), vec![(focused, true), (next, true)]);
    log.borrow_mut().clear();
    tree.dispatch_input(tab(true)).unwrap();
    assert_eq!(tree.focused(), Some(focused));
    assert_eq!(visibility(&log), vec![(focused, true)]);
    log.borrow_mut().clear();
    tree.request_focus(Some(next)).unwrap();
    assert_eq!(visibility(&log), vec![(next, true)]);
}
#[test]
fn routing_phases_and_receiver_coordinates_match_nested_paint_transforms() {
    let (mut tree, log) = setup();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(20.0, 35.0)))
        .unwrap();
    let routed: Vec<_> = log
        .borrow()
        .iter()
        .filter_map(|record| match record {
            Record::Event(context, WidgetEvent::Input(InputEvent::PointerMoved(point))) => {
                Some((context.id, context.phase, *point, context.local_position))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        routed,
        vec![
            (
                NodeId(1),
                EventPhase::Capture,
                Point::new(10.0, 15.0),
                Some(Point::new(10.0, 15.0))
            ),
            (
                NodeId(2),
                EventPhase::Capture,
                Point::new(5.0, 8.0),
                Some(Point::new(5.0, 8.0))
            ),
            (
                NodeId(3),
                EventPhase::Target,
                Point::new(2.0, 4.0),
                Some(Point::new(2.0, 4.0))
            ),
            (
                NodeId(2),
                EventPhase::Bubble,
                Point::new(5.0, 8.0),
                Some(Point::new(5.0, 8.0))
            ),
            (
                NodeId(1),
                EventPhase::Bubble,
                Point::new(10.0, 15.0),
                Some(Point::new(10.0, 15.0))
            ),
        ]
    );
    assert_eq!(tree.hovered(), Some(NodeId(3)));
}
#[test]
fn stop_propagation_is_distinct_from_handled_and_tab_navigation_wraps() {
    let (mut tree, log) = setup();
    tree.update_widget::<Probe>(NodeId(1), Invalidation::None, |p| p.stop = true)
        .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(20.0, 35.0)))
        .unwrap();
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|r| matches!(r, Record::Event(c,_) if c.phase != EventPhase::Direct))
            .count(),
        1
    );
    tree.update_widget::<Probe>(NodeId(1), Invalidation::None, |p| p.stop = false)
        .unwrap();
    assert!(tree.dispatch_input(tab(false)).unwrap().handled);
    assert_eq!(tree.focused(), Some(NodeId(3)));
    tree.update_widget::<Probe>(NodeId(3), Invalidation::None, |p| p.consume_tab = true)
        .unwrap();
    log.borrow_mut().clear();
    assert!(tree.dispatch_input(tab(false)).unwrap().handled);
    assert_eq!(tree.focused(), Some(NodeId(3)));
    assert!(
        log.borrow()
            .iter()
            .any(|r| matches!(r, Record::Event(c,_) if c.phase == EventPhase::Bubble))
    );
    tree.update_widget::<Probe>(NodeId(3), Invalidation::None, |p| p.consume_tab = false)
        .unwrap();
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(4)));
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(3)));
    tree.dispatch_input(tab(true)).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(4)));
}
#[test]
fn capture_routes_outside_bounds_and_lasts_until_all_buttons_are_released() {
    let (mut tree, log) = setup();
    tree.dispatch_input(button(PointerButton::Primary, true, Point::new(20.0, 35.0)))
        .unwrap();
    assert_eq!(tree.focused(), Some(NodeId(3)));
    assert_eq!(tree.pointer_capture(), Some(NodeId(3)));
    tree.dispatch_input(button(
        PointerButton::Secondary,
        true,
        Point::new(250.0, 150.0),
    ))
    .unwrap();
    assert_eq!(
        tree.dispatch_input(InputEvent::PointerMoved(Point::new(250.0, 150.0)))
            .unwrap()
            .target,
        Some(NodeId(3))
    );
    tree.dispatch_input(button(
        PointerButton::Primary,
        false,
        Point::new(250.0, 150.0),
    ))
    .unwrap();
    assert_eq!(tree.pointer_capture(), Some(NodeId(3)));
    tree.dispatch_input(button(
        PointerButton::Secondary,
        false,
        Point::new(250.0, 150.0),
    ))
    .unwrap();
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|r| matches!(r, Record::Event(_, WidgetEvent::PointerCaptureLost)))
            .count(),
        1
    );
}
#[test]
fn removal_notifies_before_unmount_and_cannot_leave_stale_input_owners() {
    let (mut tree, log) = setup();
    tree.dispatch_input(button(PointerButton::Primary, true, Point::new(20.0, 35.0)))
        .unwrap();
    log.borrow_mut().clear();
    tree.reconcile(description(&log, false)).unwrap();
    assert_eq!(tree.focused(), None);
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(tree.hovered(), None);
    let records = log.borrow();
    let unmount = records
        .iter()
        .position(|r| matches!(r, Record::Unmount(NodeId(3))))
        .unwrap();
    assert!(
        records[..unmount]
            .iter()
            .any(|r| matches!(r, Record::Event(_, WidgetEvent::FocusChanged(false))))
    );
    assert!(
        records[..unmount]
            .iter()
            .any(|r| matches!(r, Record::Event(_, WidgetEvent::PointerCaptureLost)))
    );
    drop(records);
    assert!(matches!(
        tree.dispatch_input(tab(false)),
        Err(TreeError::LayoutRequired)
    ));
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(4)));
    tree.remove(NodeId(2)).unwrap();
    assert_eq!(tree.focused(), None);
}
#[test]
fn cancellation_and_focusability_changes_clear_state_and_bad_input_is_rejected() {
    let (mut tree, log) = setup();
    assert!(matches!(
        tree.dispatch_input(InputEvent::PointerMoved(Point::new(f64::NAN, 0.0))),
        Err(TreeError::InvalidInput)
    ));
    assert!(log.borrow().is_empty());
    assert!(matches!(
        tree.dispatch_input(InputEvent::Touch {
            id: 1,
            phase: TouchPhase::Started,
            position: Point::new(0.0, 0.0)
        }),
        Err(TreeError::UnsupportedInput)
    ));
    tree.dispatch_input(button(PointerButton::Primary, true, Point::new(20.0, 35.0)))
        .unwrap();
    tree.update_widget::<Probe>(NodeId(3), Invalidation::None, |p| p.focusable = false)
        .unwrap();
    assert_eq!(tree.focused(), None);
    tree.cancel_input();
    assert_eq!(tree.pointer_capture(), None);
    assert_eq!(tree.hovered(), None);
    assert!(
        tree.dispatch_input(InputEvent::ImeCommit("中文".into()))
            .unwrap()
            .target
            .is_none()
    );
}

#[test]
fn keyboard_and_ime_target_focus_independently_of_pointer_hover() {
    let (mut tree, log) = setup();
    tree.dispatch_input(tab(false)).unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(50.0, 35.0)))
        .unwrap();
    assert_eq!(tree.hovered(), Some(NodeId(4)));
    assert_eq!(tree.focused(), Some(NodeId(3)));
    log.borrow_mut().clear();
    let event = InputEvent::ImePreedit {
        text: "中文".into(),
        cursor: Some((0, 6)),
    };
    let result = tree.dispatch_input(event.clone()).unwrap();
    assert_eq!(result.target, Some(NodeId(3)));
    assert!(log.borrow().iter().any(|r| matches!(r, Record::Event(c,WidgetEvent::Input(e)) if c.phase == EventPhase::Target && c.id == NodeId(3) && c.local_position.is_none() && *e == event)));
    tree.cancel_input();
    assert!(
        tree.dispatch_input(InputEvent::Key(KeyInput {
            key: Key::Enter,
            pressed: true,
            repeat: false,
            text: None,
            modifiers: Default::default()
        }))
        .unwrap()
        .target
        .is_none()
    );
}

#[test]
fn keyed_reordering_preserves_focus_and_capture_until_explicit_cancellation() {
    let (mut tree, log) = setup();
    tree.dispatch_input(button(PointerButton::Primary, true, Point::new(20.0, 35.0)))
        .unwrap();
    log.borrow_mut().clear();
    let mut view = description(&log, true);
    view.children[0].children.reverse();
    let report = tree.reconcile(view).unwrap();
    assert_eq!((report.created, report.removed), (0, 0));
    assert_eq!(tree.focused(), Some(NodeId(3)));
    assert_eq!(tree.pointer_capture(), Some(NodeId(3)));
    assert!(log.borrow().is_empty());
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    assert_eq!(
        tree.dispatch_input(InputEvent::PointerMoved(Point::new(250.0, 150.0)))
            .unwrap()
            .target,
        Some(NodeId(3))
    );
    tree.cancel_input();
    assert_eq!(tree.focused(), None);
    assert_eq!(tree.pointer_capture(), None);
}

#[test]
fn event_geometry_is_local_for_each_receiver_before_first_frame() {
    use zui_api::layout::Edges;
    let log = Log::default();
    let mut view = description(&log, true);
    view.style.padding = Edges::all(2.0);
    view.children[0].style.padding = Edges::all(3.0);
    view.children[0].children[0].style.padding = Edges {
        left: 4.0,
        right: 5.0,
        top: 6.0,
        bottom: 7.0,
    };
    let mut tree = Tree::from_view(view, TreeLimits::default()).unwrap();
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    // Input must work after preparation, without a scene/paint call.
    tree.dispatch_input(button(PointerButton::Primary, true, Point::new(20.0, 35.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(250.0, 150.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::ImeCommit("字".into()))
        .unwrap();
    let mut phases = Vec::new();
    for record in log.borrow().iter() {
        let Record::Event(context, _) = record else {
            continue;
        };
        if context.phase == EventPhase::Direct {
            assert_eq!(context.geometry, None);
            continue;
        }
        let (size, content_bounds) = match context.id.0 {
            1 => (Size::new(200.0, 100.0), Rect::new(2.0, 2.0, 196.0, 96.0)),
            2 => (Size::new(100.0, 80.0), Rect::new(3.0, 3.0, 94.0, 74.0)),
            3 => (Size::new(20.0, 20.0), Rect::new(4.0, 6.0, 11.0, 7.0)),
            _ => panic!("unexpected receiver"),
        };
        assert_eq!(
            context.geometry,
            Some(EventGeometry {
                size,
                content_bounds
            })
        );
        phases.push(context.phase);
    }
    assert!(phases.contains(&EventPhase::Capture));
    assert!(phases.contains(&EventPhase::Target));
    assert!(phases.contains(&EventPhase::Bubble));

    log.borrow_mut().clear();
    tree.invalidate(Invalidation::Layout);
    tree.cancel_input();
    assert!(!log.borrow().is_empty());
    assert!(log.borrow().iter().all(|record| matches!(record,
        Record::Event(context, _) if context.phase == EventPhase::Direct && context.geometry.is_none()
    )));
    assert!(matches!(
        tree.dispatch_input(tab(false)),
        Err(TreeError::LayoutRequired)
    ));
}

#[test]
fn event_geometry_and_paint_share_padding_clamping_after_relayout() {
    use zui_api::layout::Edges;
    struct GeometryProbe {
        events: Rc<RefCell<Vec<EventGeometry>>>,
        paints: Rc<RefCell<Vec<PaintContext>>>,
    }
    impl Widget for GeometryProbe {
        fn paint(&self, context: PaintContext, _: &mut Painter<'_>) {
            self.paints.borrow_mut().push(context);
        }
        fn event(&mut self, context: EventContext, _: &WidgetEvent) -> EventResponse {
            if let Some(geometry) = context.geometry {
                self.events.borrow_mut().push(geometry);
            }
            EventResponse::default()
        }
    }
    let events = Rc::new(RefCell::new(Vec::new()));
    let paints = Rc::new(RefCell::new(Vec::new()));
    let mut tree = Tree::new(
        GeometryProbe {
            events: events.clone(),
            paints: paints.clone(),
        },
        LayoutStyle::default(),
        TreeLimits::default(),
    )
    .unwrap();
    for padding in [Edges::all(3.0), Edges::all(150.0)] {
        tree.set_style(
            tree.root(),
            LayoutStyle {
                padding,
                ..Default::default()
            },
        )
        .unwrap();
        tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
        tree.dispatch_input(InputEvent::PointerMoved(Point::new(20.0, 35.0)))
            .unwrap();
        tree.scene().unwrap();
        let geometry = *events.borrow().last().unwrap();
        let paint = *paints.borrow().last().unwrap();
        assert_eq!(geometry.size, paint.size);
        assert_eq!(geometry.content_bounds, paint.content_bounds);
    }
    let events = events.borrow();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].content_bounds, Rect::new(3.0, 3.0, 194.0, 94.0));
    assert_eq!(events[1].content_bounds, Rect::new(150.0, 150.0, 0.0, 0.0));
}

#[test]
fn text_input_capability_uses_focus_transforms_clips_and_validated_geometry() {
    use zui_api::{layout::Edges, widget::TextInputState};
    #[derive(Clone)]
    struct ImeProbe {
        area: Rect,
        generation: u64,
        disabled: bool,
    }
    impl Widget for ImeProbe {
        fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
        fn enabled(&self) -> bool {
            !self.disabled
        }
        fn focusable(&self) -> bool {
            true
        }
        fn text_input(&self, context: PaintContext) -> Option<TextInputState> {
            Some(TextInputState {
                generation: self.generation,
                cursor_area: Rect::new(
                    context.content_bounds.origin.x + self.area.origin.x,
                    context.content_bounds.origin.y + self.area.origin.y,
                    self.area.size.width,
                    self.area.size.height,
                ),
            })
        }
    }
    impl ViewSpec for ImeProbe {
        type Widget = Self;
        fn build(&self) -> Self {
            self.clone()
        }
        fn update(&self, widget: &mut Self) -> Invalidation {
            *widget = self.clone();
            Invalidation::Paint
        }
    }
    let log = Log::default();
    let mut view = description(&log, true);
    view.children[0].children[0] = View::new(ImeProbe {
        area: Rect::new(16.0, 4.0, 8.0, 30.0),
        generation: 7,
        disabled: false,
    })
    .keyed("leaf")
    .clipped(true)
    .styled(LayoutStyle {
        padding: Edges {
            left: 2.0,
            top: 3.0,
            ..Default::default()
        },
        ..Default::default()
    });
    let mut tree = Tree::from_view(view, TreeLimits::default()).unwrap();
    assert_eq!(tree.text_input(), Err(TreeError::LayoutRequired));
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    assert_eq!(tree.text_input().unwrap(), None);
    tree.dispatch_input(tab(false)).unwrap();
    let target = tree.text_input().unwrap().unwrap();
    assert_eq!(target.id, NodeId(3));
    assert_eq!(target.generation, 7);
    assert_eq!(target.cursor_area, Rect::new(36.0, 38.0, 2.0, 13.0));
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.text_input().unwrap(), None);
    tree.dispatch_input(tab(false)).unwrap();
    tree.update_widget::<ImeProbe>(NodeId(3), Invalidation::Paint, |p| p.area.origin.x = 100.0)
        .unwrap();
    assert_eq!(tree.text_input().unwrap(), None);
    tree.update_widget::<ImeProbe>(NodeId(3), Invalidation::Paint, |p| {
        p.area.origin.x = f64::NAN
    })
    .unwrap();
    assert_eq!(
        tree.text_input(),
        Err(TreeError::InvalidTextInputArea(NodeId(3)))
    );
    tree.update_widget::<ImeProbe>(NodeId(3), Invalidation::Paint, |p| p.disabled = true)
        .unwrap();
    assert_eq!(tree.focused(), None);
    assert_eq!(tree.text_input().unwrap(), None);
}

#[test]
fn partial_reconcile_preserves_retained_input_and_cancels_removed_owners() {
    let (mut tree, log) = setup();
    tree.dispatch_input(button(PointerButton::Primary, true, Point::new(20.0, 35.0)))
        .unwrap();
    let mut view = description(&log, true);
    let branch = view.children.pop().unwrap();
    tree.reconcile_children(NodeId(2), branch.children).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(3)));
    assert_eq!(tree.pointer_capture(), Some(NodeId(3)));
    log.borrow_mut().clear();
    let mut view = description(&log, false);
    let branch = view.children.pop().unwrap();
    tree.reconcile_children(NodeId(2), branch.children).unwrap();
    assert_eq!(tree.focused(), None);
    assert_eq!(tree.pointer_capture(), None);
    let records = log.borrow();
    let unmount = records
        .iter()
        .position(|r| matches!(r, Record::Unmount(NodeId(3))))
        .unwrap();
    assert!(
        records[..unmount]
            .iter()
            .any(|r| matches!(r, Record::Event(_, WidgetEvent::PointerCaptureLost)))
    );
    assert!(
        records[..unmount]
            .iter()
            .any(|r| matches!(r, Record::Event(_, WidgetEvent::FocusChanged(false))))
    );
    assert!(!tree.contains(NodeId(3)));
    assert_eq!(tree.children(NodeId(2)).unwrap(), &[NodeId(4)]);
}

#[test]
fn programmatic_focus_validates_targets_is_idempotent_and_preserves_capture() {
    let (mut tree, log) = setup();
    tree.dispatch_input(button(PointerButton::Primary, true, Point::new(20.0, 35.0)))
        .unwrap();
    log.borrow_mut().clear();
    assert!(tree.request_focus(Some(NodeId(4))).unwrap());
    assert_eq!(tree.focused(), Some(NodeId(4)));
    assert_eq!(tree.pointer_capture(), Some(NodeId(3)));
    let changes: Vec<_> = log
        .borrow()
        .iter()
        .filter_map(|record| match record {
            Record::Event(context, WidgetEvent::FocusChanged(focused)) => {
                Some((context.id, *focused, context.phase))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        changes,
        vec![
            (NodeId(3), false, EventPhase::Direct),
            (NodeId(4), true, EventPhase::Direct)
        ]
    );
    let calls = log.borrow().len();
    assert!(!tree.request_focus(Some(NodeId(4))).unwrap());
    assert_eq!(
        tree.request_focus(Some(NodeId(999))),
        Err(TreeError::MissingNode(NodeId(999)))
    );
    assert_eq!(
        tree.request_focus(Some(NodeId(2))),
        Err(TreeError::NotFocusable(NodeId(2)))
    );
    assert_eq!(tree.focused(), Some(NodeId(4)));
    assert_eq!(log.borrow().len(), calls);
    assert!(tree.request_focus(None).unwrap());
    assert!(!tree.request_focus(None).unwrap());
    assert_eq!(tree.pointer_capture(), Some(NodeId(3)));
}

#[test]
fn programmatic_focus_works_before_layout_and_rejects_disabled_nodes() {
    struct Disabled;
    impl Widget for Disabled {
        fn enabled(&self) -> bool {
            false
        }
        fn focusable(&self) -> bool {
            true
        }
        fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    }
    let log = Log::default();
    let mut tree = Tree::from_view(description(&log, true), TreeLimits::default()).unwrap();
    assert!(tree.request_focus(Some(NodeId(3))).unwrap());
    assert_eq!(tree.focused(), Some(NodeId(3)));
    assert_eq!(tree.invalidation(), Invalidation::Layout);
    let disabled = tree
        .insert(NodeId(2), 2, Disabled, LayoutStyle::default())
        .unwrap();
    assert_eq!(
        tree.request_focus(Some(disabled)),
        Err(TreeError::NotFocusable(disabled))
    );
    assert_eq!(tree.focused(), Some(NodeId(3)));
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    tree.scene().unwrap();
    tree.remove(NodeId(3)).unwrap();
    assert_eq!(tree.focused(), None);
    assert_eq!(
        tree.request_focus(Some(NodeId(3))),
        Err(TreeError::MissingNode(NodeId(3)))
    );
}

type EntryLog = Rc<RefCell<Vec<(u64, FocusEntry)>>>;
type NavigationLog = Rc<RefCell<Vec<(u64, FocusNavigation)>>>;

#[derive(Clone)]
struct Navigator {
    entry_log: EntryLog,
    intercept_entry: bool,
    name: u64,
    leaf: bool,
    intercept: bool,
    consume_key: bool,
    disabled: bool,
    log: NavigationLog,
}
impl ViewSpec for Navigator {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        *widget = self.clone();
        Invalidation::None
    }
}
impl Widget for Navigator {
    fn enabled(&self) -> bool {
        !self.disabled
    }
    fn focusable(&self) -> bool {
        self.leaf
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn event(&mut self, _: EventContext, _: &WidgetEvent) -> EventResponse {
        EventResponse {
            handled: self.consume_key,
            ..Default::default()
        }
    }
    fn enter_focus(&mut self, entry: FocusEntry) -> FocusNavigationResponse {
        self.entry_log.borrow_mut().push((self.name, entry));
        FocusNavigationResponse {
            handled: self.intercept_entry,
            actions: if self.intercept_entry {
                vec![zui_api::action::WidgetAction::new(
                    zui_api::action::ActionId(43),
                    entry.reverse,
                )]
            } else {
                vec![]
            },
            ..Default::default()
        }
    }
    fn navigate_focus(&mut self, navigation: FocusNavigation) -> FocusNavigationResponse {
        self.log.borrow_mut().push((self.name, navigation));
        FocusNavigationResponse {
            handled: self.intercept,
            invalidation: if self.intercept {
                Invalidation::Paint
            } else {
                Invalidation::None
            },
            actions: if self.intercept {
                vec![zui_api::action::WidgetAction::new(
                    zui_api::action::ActionId(42),
                    navigation.reverse,
                )]
            } else {
                vec![]
            },
        }
    }
}
fn navigation_tree() -> (Tree, NavigationLog) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let node = |name, leaf| {
        View::new(Navigator {
            entry_log: Default::default(),
            intercept_entry: false,
            name,
            leaf,
            intercept: false,
            consume_key: false,
            disabled: false,
            log: log.clone(),
        })
    };
    let container = |view: View| {
        view.styled(LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        })
    };
    let view = container(node(1, false)).with_children([
        container(node(2, false)).with_children([node(3, true), node(4, true)]),
        node(5, true),
    ]);
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    (tree, log)
}
#[test]
fn navigation_reports_direct_branches_and_nearest_handler_owns_default() {
    let (mut tree, log) = navigation_tree();
    tree.request_focus(Some(NodeId(3))).unwrap();
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(4)));
    let calls = log.borrow().clone();
    assert_eq!(
        calls.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert_eq!(calls[1].1.focused_child, Some(NodeId(3)));
    assert_eq!(calls[1].1.candidate_child, Some(NodeId(4)));
    assert_eq!(calls[2].1.focused_child, Some(NodeId(2)));
    assert_eq!(calls[2].1.candidate_child, Some(NodeId(2)));
    log.borrow_mut().clear();
    tree.update_widget::<Navigator>(NodeId(2), Invalidation::None, |n| n.intercept = true)
        .unwrap();
    let result = tree.dispatch_input(tab(false)).unwrap();
    assert!(result.handled);
    assert_eq!(tree.focused(), Some(NodeId(4)));
    assert_eq!(result.actions.len(), 1);
    assert_eq!(result.actions[0].source, NodeId(2));
    assert_eq!(result.actions[0].action.payload::<bool>(), Some(&false));
    let calls = log.borrow().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].1.candidate, Some(NodeId(5)));
    assert_eq!(calls[1].1.candidate_child, None);
    log.borrow_mut().clear();
    tree.request_focus(Some(NodeId(3))).unwrap();
    tree.dispatch_input(tab(true)).unwrap();
    assert!(log.borrow()[1].1.reverse);
    assert_eq!(log.borrow()[1].1.candidate, Some(NodeId(5)));
    assert_eq!(tree.focused(), Some(NodeId(3)));
}
#[test]
fn navigation_skips_consumed_modified_released_tabs_and_disabled_receivers() {
    let (mut tree, log) = navigation_tree();
    tree.dispatch_input(tab(false)).unwrap();
    assert!(log.borrow().is_empty()); // no current focus: use ordinary entry
    tree.update_widget::<Navigator>(NodeId(3), Invalidation::None, |n| n.consume_key = true)
        .unwrap();
    tree.dispatch_input(tab(false)).unwrap();
    assert!(log.borrow().is_empty());
    tree.update_widget::<Navigator>(NodeId(3), Invalidation::None, |n| n.consume_key = false)
        .unwrap();
    for variant in 0..4 {
        let InputEvent::Key(mut key) = tab(false) else {
            unreachable!()
        };
        match variant {
            0 => key.pressed = false,
            1 => key.modifiers.control = true,
            2 => key.modifiers.alt = true,
            _ => key.modifiers.super_key = true,
        }
        tree.dispatch_input(InputEvent::Key(key)).unwrap();
    }
    assert!(log.borrow().is_empty());
    tree.update_widget::<Navigator>(NodeId(2), Invalidation::None, |n| {
        n.disabled = true;
        n.intercept = true;
    })
    .unwrap();
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(
        log.borrow()
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        vec![3, 1]
    );
    assert_eq!(tree.focused(), Some(NodeId(4)));
}

#[test]
fn focus_entry_asks_only_new_path_outermost_first_and_can_defer_default() {
    let (mut tree, _) = navigation_tree();
    let log = EntryLog::default();
    for id in 1..=5 {
        tree.update_widget::<Navigator>(NodeId(id), Invalidation::None, |n| {
            n.entry_log = log.clone()
        })
        .unwrap();
    }
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(
        log.borrow().iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(log.borrow()[0].1.focused, None);
    assert_eq!(log.borrow()[0].1.candidate_child, Some(NodeId(2)));
    log.borrow_mut().clear();
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(
        log.borrow().iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![4]
    );
    assert_eq!(log.borrow()[0].1.candidate_child, None);
    tree.request_focus(Some(NodeId(5))).unwrap();
    tree.update_widget::<Navigator>(NodeId(2), Invalidation::None, |n| n.intercept_entry = true)
        .unwrap();
    log.borrow_mut().clear();
    let result = tree.dispatch_input(tab(true)).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(5)));
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(log.borrow()[0].0, 2);
    assert_eq!(log.borrow()[0].1.focused, Some(NodeId(5)));
    assert_eq!(log.borrow()[0].1.candidate, NodeId(4));
    assert_eq!(log.borrow()[0].1.candidate_child, Some(NodeId(4)));
    assert!(result.handled);
    assert_eq!(result.actions[0].source, NodeId(2));
    assert_eq!(result.actions[0].action.payload::<bool>(), Some(&true));
    log.borrow_mut().clear();
    tree.update_widget::<Navigator>(NodeId(2), Invalidation::None, |n| n.disabled = true)
        .unwrap();
    tree.dispatch_input(tab(true)).unwrap();
    assert_eq!(tree.focused(), Some(NodeId(4)));
    assert_eq!(
        log.borrow().iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![4]
    );
}

#[derive(Clone)]
struct DirectedFocus {
    request: Rc<std::cell::Cell<FocusRequest>>,
    enabled: Rc<std::cell::Cell<bool>>,
    focusable: Rc<std::cell::Cell<bool>>,
    phase: EventPhase,
    disable: Option<Rc<std::cell::Cell<bool>>>,
    log: Log,
}
impl Widget for DirectedFocus {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn enabled(&self) -> bool {
        self.enabled.get()
    }
    fn focusable(&self) -> bool {
        self.focusable.get()
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        self.log
            .borrow_mut()
            .push(Record::Event(context, event.clone()));
        if matches!(event, WidgetEvent::Input(InputEvent::Key(_))) && context.phase == self.phase {
            if let Some(enabled) = &self.disable {
                enabled.set(false);
            }
            EventResponse {
                focus: self.request.get(),
                handled: true,
                ..Default::default()
            }
        } else {
            EventResponse::default()
        }
    }
}
impl ViewSpec for DirectedFocus {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        *widget = self.clone();
        Invalidation::None
    }
}
fn directed(log: &Log, phase: EventPhase) -> DirectedFocus {
    DirectedFocus {
        request: Rc::new(std::cell::Cell::new(FocusRequest::None)),
        enabled: Rc::new(std::cell::Cell::new(true)),
        focusable: Rc::new(std::cell::Cell::new(true)),
        phase,
        disable: None,
        log: log.clone(),
    }
}
#[test]
fn directed_focus_is_deferred_first_valid_wins_and_notifications_follow_route() {
    let log = Log::default();
    let root = directed(&log, EventPhase::Capture);
    let source = directed(&log, EventPhase::Target);
    let target = directed(&log, EventPhase::Target);
    let mut tree = Tree::from_view(
        View::new(root.clone())
            .styled(LayoutStyle {
                kind: LayoutKind::Column,
                ..Default::default()
            })
            .with_children([View::new(source.clone()), View::new(target)]),
        Default::default(),
    )
    .unwrap();
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    let source_id = tree.children(tree.root()).unwrap()[0];
    let target_id = tree.children(tree.root()).unwrap()[1];
    tree.request_focus(Some(source_id)).unwrap();
    log.borrow_mut().clear();
    root.request.set(FocusRequest::Target(target_id));
    source.request.set(FocusRequest::Focus);
    let result = tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(result.target, Some(source_id));
    assert_eq!(tree.focused(), Some(target_id));
    assert_eq!(tree.pointer_capture(), None);
    let records = log.borrow();
    let notifications: Vec<_> = records
        .iter()
        .filter_map(|record| match record {
            Record::Event(ctx, WidgetEvent::FocusChanged(value)) => Some((ctx.id, *value)),
            _ => None,
        })
        .collect();
    assert_eq!(notifications, [(source_id, false), (target_id, true)]);
    let last_input = records
        .iter()
        .rposition(|record| matches!(record, Record::Event(_, WidgetEvent::Input(_))))
        .unwrap();
    let first_focus = records
        .iter()
        .position(|record| matches!(record, Record::Event(_, WidgetEvent::FocusChanged(_))))
        .unwrap();
    assert!(last_input < first_focus);
}
#[test]
fn invalid_directed_requests_allow_later_valid_request_and_stale_ids_are_ignored() {
    let log = Log::default();
    let root = directed(&log, EventPhase::Capture);
    let source = directed(&log, EventPhase::Target);
    let target = directed(&log, EventPhase::Target);
    let mut tree = Tree::from_view(
        View::new(root.clone())
            .styled(LayoutStyle {
                kind: LayoutKind::Column,
                ..Default::default()
            })
            .with_children([View::new(source.clone()), View::new(target.clone())]),
        Default::default(),
    )
    .unwrap();
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    let source_id = tree.children(tree.root()).unwrap()[0];
    let target_id = tree.children(tree.root()).unwrap()[1];
    root.request.set(FocusRequest::Target(NodeId(u64::MAX)));
    source.request.set(FocusRequest::Target(target_id));
    tree.request_focus(Some(source_id)).unwrap();
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(target_id));
    tree.request_focus(Some(source_id)).unwrap();
    target.enabled.set(false);
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(source_id));
    target.enabled.set(true);
    target.focusable.set(false);
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(source_id));
    source.request.set(FocusRequest::Target(NodeId(u64::MAX)));
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(source_id));
}
#[test]
fn directed_focus_revalidates_after_later_callback_disables_target() {
    let log = Log::default();
    let root = directed(&log, EventPhase::Capture);
    let mut source = directed(&log, EventPhase::Target);
    let target = directed(&log, EventPhase::Target);
    source.disable = Some(target.enabled.clone());
    let mut tree = Tree::from_view(
        View::new(root.clone())
            .styled(LayoutStyle {
                kind: LayoutKind::Column,
                ..Default::default()
            })
            .with_children([View::new(source), View::new(target)]),
        Default::default(),
    )
    .unwrap();
    tree.layout(&mut Layout, Size::new(300.0, 200.0)).unwrap();
    let source_id = tree.children(tree.root()).unwrap()[0];
    let target_id = tree.children(tree.root()).unwrap()[1];
    root.request.set(FocusRequest::Target(target_id));
    tree.request_focus(Some(source_id)).unwrap();
    log.borrow_mut().clear();
    tree.dispatch_input(tab(false)).unwrap();
    assert_eq!(tree.focused(), Some(source_id));
    assert!(
        !log.borrow()
            .iter()
            .any(|record| matches!(record, Record::Event(_, WidgetEvent::FocusChanged(_))))
    );
}
