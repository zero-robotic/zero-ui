use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zui_api::{layout::*, view::*, widget::*};
use zui_core::{Tree, TreeError, TreeLimits};
use zui_paint::Painter;
use zui_types::{Point, Rect, Size};

#[derive(Clone)]
struct Row(Rc<Cell<usize>>, Rc<Cell<bool>>);
impl Widget for Row {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        if self.1.get() {
            return Err(LayoutError::MeasurementFailed(context.id));
        }
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}
impl ViewSpec for Row {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
type Log = Rc<RefCell<Vec<Vec<ChildLayout>>>>;
#[derive(Clone)]
struct Owner {
    height: f32,
    tag_revision: Rc<Cell<u64>>,
    log: Log,
    prepared: Rc<Cell<usize>>,
    prepare_error: Rc<Cell<bool>>,
    feedback_error: Rc<Cell<bool>>,
    unstable: Rc<Cell<bool>>,
}
impl Widget for Owner {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn child_viewport(&self, _: PaintContext) -> ChildViewport {
        ChildViewport {
            translation: Point::new(0.0, -100.0),
            clip: None,
        }
    }
    fn child_views(&self, _: PaintContext) -> Result<Vec<View>, LayoutError> {
        Ok([7_u64, 9]
            .into_iter()
            .map(|key| {
                View::new(Row(self.prepared.clone(), self.prepare_error.clone()))
                    .keyed(key)
                    .layout_tagged(LayoutTag {
                        revision: self.tag_revision.get(),
                        value: key,
                    })
                    .styled(LayoutStyle {
                        height: Dimension::Length(self.height),
                        ..Default::default()
                    })
            })
            .collect())
    }
    fn children_prepared(
        &mut self,
        context: PaintContext,
        children: &[ChildLayout],
    ) -> Result<Invalidation, LayoutError> {
        assert_eq!(context.content_bounds.size, Size::new(100.0, 80.0));
        assert!(
            self.prepared.get() >= children.len(),
            "all child prepare callbacks must precede feedback"
        );
        self.log.borrow_mut().push(children.to_vec());
        if self.feedback_error.get() {
            return Err(LayoutError::MeasurementFailed(context.id));
        }
        if self.unstable.get() {
            return Ok(Invalidation::Layout);
        }
        if !children.is_empty() && self.height == 10.0 {
            self.height = 20.0;
            return Ok(Invalidation::Paint); // changed descriptions will request layout
        }
        Ok(Invalidation::None)
    }
}
impl ViewSpec for Owner {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
fn owner() -> Owner {
    Owner {
        height: 10.0,
        tag_revision: Rc::default(),
        log: Rc::default(),
        prepared: Rc::default(),
        prepare_error: Rc::default(),
        feedback_error: Rc::default(),
        unstable: Rc::default(),
    }
}
fn tree(owner: &Owner, managed: bool) -> Tree {
    let mut view = View::new(owner.clone()).styled(LayoutStyle {
        kind: LayoutKind::Column,
        ..Default::default()
    });
    if managed {
        view = view.managed_children();
    }
    Tree::from_view(view, TreeLimits::default()).unwrap()
}
struct Layout {
    calls: usize,
    incomplete: bool,
}
impl LayoutEngine for Layout {
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        root: NodeId,
        _: Size,
    ) -> Result<(), LayoutError> {
        self.calls += 1;
        tree.set_layout(root, LayoutBox::new(Rect::new(30.0, 40.0, 100.0, 80.0)));
        if !self.incomplete {
            let children = tree.node(root).unwrap().children.to_vec();
            for (i, id) in children.into_iter().enumerate() {
                let Dimension::Length(height) = tree.node(id).unwrap().style.height else {
                    panic!()
                };
                tree.set_layout(
                    id,
                    LayoutBox::new(Rect::new(4.0, 6.0 + i as f64 * 30.0, 50.0, height as f64)),
                );
            }
        }
        Ok(())
    }
}
fn layout() -> Layout {
    Layout {
        calls: 0,
        incomplete: false,
    }
}
const SIZE: Size = Size::new(100.0, 80.0);

#[test]
fn feedback_uses_prepared_ordered_local_geometry_and_converges_changed_descriptions() {
    let props = owner();
    let mut tree = tree(&props, true);
    let mut engine = layout();
    tree.layout(&mut engine, SIZE).unwrap();
    assert_eq!(engine.calls, 3);
    let log = props.log.borrow();
    assert_eq!(log.len(), 3);
    assert!(log[0].is_empty());
    for (pass, height) in [(1, 10.0), (2, 20.0)] {
        assert_eq!(
            log[pass]
                .iter()
                .map(|child| child.key.clone())
                .collect::<Vec<_>>(),
            vec![Some(Key::Number(7)), Some(Key::Number(9))]
        );
        assert_eq!(
            log[pass][0].layout.bounds,
            Rect::new(4.0, 6.0, 50.0, height)
        );
        assert_eq!(log[pass][1].layout.bounds.origin, Point::new(4.0, 36.0));
    }
    assert_eq!(log[1][0].id, log[2][0].id);
    drop(log);
    tree.scene().unwrap();
    tree.invalidate(Invalidation::Paint);
    assert!(!tree.layout(&mut engine, SIZE).unwrap());
    assert_eq!(props.log.borrow().len(), 3);
    assert_eq!(engine.calls, 3);
}
#[test]
fn feedback_failure_blocks_scene_and_retries_without_replacing_children() {
    let props = owner();
    let mut tree = tree(&props, true);
    let mut engine = layout();
    tree.layout(&mut engine, SIZE).unwrap();
    let ids = tree.children(tree.root()).unwrap().to_vec();
    props.feedback_error.set(true);
    tree.invalidate(Invalidation::Layout);
    assert!(matches!(
        tree.layout(&mut engine, SIZE),
        Err(TreeError::Layout(LayoutError::MeasurementFailed(_)))
    ));
    assert!(matches!(tree.scene(), Err(TreeError::LayoutRequired)));
    assert_eq!(tree.children(tree.root()).unwrap(), ids);
    props.feedback_error.set(false);
    tree.layout(&mut engine, SIZE).unwrap();
    assert!(tree.scene().is_ok());
    assert_eq!(tree.children(tree.root()).unwrap(), ids);
}
#[test]
fn incomplete_geometry_and_failed_prepare_never_deliver_feedback() {
    let props = owner();
    let mut tree = tree(&props, true);
    let mut engine = layout();
    tree.layout(&mut engine, SIZE).unwrap();
    let calls = props.log.borrow().len();
    engine.incomplete = true;
    tree.invalidate(Invalidation::Layout);
    assert_eq!(
        tree.layout(&mut engine, SIZE),
        Err(TreeError::IncompleteLayout)
    );
    assert_eq!(props.log.borrow().len(), calls);
    engine.incomplete = false;
    props.prepare_error.set(true);
    assert!(matches!(
        tree.layout(&mut engine, SIZE),
        Err(TreeError::Layout(LayoutError::MeasurementFailed(_)))
    ));
    assert_eq!(props.log.borrow().len(), calls);
    props.prepare_error.set(false);
    tree.layout(&mut engine, SIZE).unwrap();
    assert_eq!(props.log.borrow().len(), calls + 1);
}
#[test]
fn feedback_is_opt_in_and_cannot_create_an_unbounded_layout_loop() {
    let props = owner();
    let mut ordinary = tree(&props, false);
    ordinary.layout(&mut layout(), SIZE).unwrap();
    assert!(props.log.borrow().is_empty());
    let mut tree = tree(&props, true);
    let mut engine = layout();
    props.unstable.set(true);
    assert_eq!(
        tree.layout(&mut engine, SIZE),
        Err(TreeError::ChildLayoutDidNotConverge)
    );
    assert_eq!(engine.calls, 16);
    assert!(matches!(tree.scene(), Err(TreeError::LayoutRequired)));
    props.unstable.set(false);
    tree.layout(&mut engine, SIZE).unwrap();
    assert!(tree.scene().is_ok());
}

#[test]
fn changed_feedback_tag_preserves_identity_and_requests_fresh_layout() {
    let props = owner();
    let mut tree = tree(&props, true);
    let mut engine = layout();
    tree.layout(&mut engine, SIZE).unwrap();
    let original = props.log.borrow().last().unwrap()[0].clone();
    assert_eq!(
        original.tag,
        Some(LayoutTag {
            revision: 0,
            value: 7
        })
    );
    let calls = engine.calls;
    props.tag_revision.set(9);
    tree.invalidate(Invalidation::Paint);
    tree.layout(&mut engine, SIZE).unwrap();
    let last = props.log.borrow().last().unwrap()[0].clone();
    assert_eq!(last.id, original.id);
    assert_eq!(
        last.tag,
        Some(LayoutTag {
            revision: 9,
            value: 7
        })
    );
    assert_eq!(engine.calls, calls + 1);
}
