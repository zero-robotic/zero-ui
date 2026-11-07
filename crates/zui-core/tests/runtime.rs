use std::{cell::RefCell, rc::Rc};
use zui_api::{layout::*, widget::*};
use zui_core::{Tree, TreeError, TreeLimits};
use zui_paint::{Command, Painter};
use zui_types::{Color, Point, Rect, Size};

type Log = Rc<RefCell<Vec<(NodeId, Lifecycle)>>>;
struct Custom {
    log: Log,
    color: Color,
    size: Size,
}
impl Custom {
    fn new(log: &Log) -> Self {
        Self {
            log: log.clone(),
            color: Color::WHITE,
            size: Size::new(20.0, 20.0),
        }
    }
}
impl Widget for Custom {
    fn lifecycle(&mut self, id: NodeId, event: Lifecycle) {
        self.log.borrow_mut().push((id, event));
    }
    fn measure(&mut self, _: MeasureContext) -> Result<Size, LayoutError> {
        Ok(self.size)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        painter.fill_rect(
            Rect::new(0.0, 0.0, context.size.width, context.size.height),
            self.color,
        );
    }
}
fn container() -> LayoutStyle {
    LayoutStyle {
        kind: LayoutKind::Column,
        ..Default::default()
    }
}
fn setup() -> (Tree, Log) {
    let log = Log::default();
    let tree = Tree::new(Custom::new(&log), container(), TreeLimits::default()).unwrap();
    (tree, log)
}
#[derive(Default)]
struct MeasureLayout {
    calls: usize,
}
impl LayoutEngine for MeasureLayout {
    fn compute(
        &mut self,
        tree: &mut dyn LayoutTree,
        root: NodeId,
        _: Size,
    ) -> Result<(), LayoutError> {
        self.calls += 1;
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            pending.extend_from_slice(tree.node(id).unwrap().children);
            let size = tree.measure(
                id,
                MeasureInput {
                    known_width: None,
                    known_height: None,
                    available_width: AvailableSpace::MaxContent,
                    available_height: AvailableSpace::MaxContent,
                },
            )?;
            tree.set_layout(
                id,
                LayoutBox::new(Rect::new(0.0, 0.0, size.width, size.height)),
            );
        }
        Ok(())
    }
}
fn viewport() -> Size {
    Size::new(300.0, 200.0)
}

#[test]
fn invalid_scroll_extent_is_rejected_before_layout_publication() {
    struct InvalidExtent(Size);
    impl LayoutEngine for InvalidExtent {
        fn compute(
            &mut self,
            tree: &mut dyn LayoutTree,
            root: NodeId,
            _: Size,
        ) -> Result<(), LayoutError> {
            tree.set_layout(
                root,
                LayoutBox {
                    bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
                    scroll_extent: self.0,
                },
            );
            Ok(())
        }
    }
    let (mut tree, _) = setup();
    for extent in [
        Size::new(f64::NAN, 20.0),
        Size::new(20.0, f64::INFINITY),
        Size::new(19.0, 20.0),
        Size::new(20.0, -1.0),
    ] {
        assert_eq!(
            tree.layout(&mut InvalidExtent(extent), viewport()),
            Err(TreeError::IncompleteLayout)
        );
        assert_eq!(tree.scene().unwrap_err(), TreeError::LayoutRequired);
    }
    tree.layout(&mut MeasureLayout::default(), viewport())
        .unwrap();
    assert!(tree.scene().is_ok());
}

#[test]
fn widget_state_survives_reorder_and_reparent_without_lifecycle_churn() {
    let (mut tree, log) = setup();
    let root = tree.root();
    let a = tree
        .insert(root, 0, Custom::new(&log), container())
        .unwrap();
    let b = tree
        .insert(root, 1, Custom::new(&log), container())
        .unwrap();
    let leaf = tree
        .insert(a, 0, Custom::new(&log), LayoutStyle::default())
        .unwrap();
    tree.update_widget::<Custom>(leaf, Invalidation::Layout, |widget| {
        widget.size = Size::new(42.0, 23.0)
    })
    .unwrap();
    tree.reorder(root, &[b, a]).unwrap();
    tree.reparent(leaf, b, 0).unwrap();
    assert_eq!(tree.parent(leaf).unwrap(), Some(b));
    tree.layout(&mut MeasureLayout::default(), viewport())
        .unwrap();
    assert_eq!(
        tree.layout_box(leaf).unwrap().bounds.size,
        Size::new(42.0, 23.0)
    );
    assert_eq!(log.borrow().len(), 4);
    tree.remove(b).unwrap();
    assert_eq!(
        &log.borrow()[4..],
        &[(leaf, Lifecycle::Unmounted), (b, Lifecycle::Unmounted)]
    );
    assert!(!tree.contains(leaf));
    let replacement = tree
        .insert(root, 0, Custom::new(&log), LayoutStyle::default())
        .unwrap();
    assert_ne!(replacement, leaf);
    assert!(matches!(
        tree.set_clip(leaf, true),
        Err(TreeError::MissingNode(_))
    ));
    drop(tree);
    let log = log.borrow();
    for id in [root, a, b, leaf, replacement] {
        assert_eq!(
            log.iter()
                .filter(|entry| **entry == (id, Lifecycle::Mounted))
                .count(),
            1
        );
        assert_eq!(
            log.iter()
                .filter(|entry| **entry == (id, Lifecycle::Unmounted))
                .count(),
            1
        );
    }
    assert_eq!(log.last(), Some(&(root, Lifecycle::Unmounted)));
}

#[test]
fn invalid_mutations_preserve_the_tree_and_do_not_mount_widgets() {
    let (mut tree, log) = setup();
    let root = tree.root();
    let a = tree
        .insert(root, 0, Custom::new(&log), container())
        .unwrap();
    let b = tree.insert(a, 0, Custom::new(&log), container()).unwrap();
    assert_eq!(tree.reparent(a, b, 0), Err(TreeError::Cycle));
    assert_eq!(tree.remove(root), Err(TreeError::RootMutation));
    assert_eq!(tree.reorder(root, &[a, a]), Err(TreeError::InvalidOrder));
    assert_eq!(tree.reorder(root, &[b]), Err(TreeError::InvalidOrder));
    assert_eq!(
        tree.set_style(a, LayoutStyle::default()),
        Err(TreeError::LeafHasChildren)
    );
    assert_eq!(
        tree.insert(root, 9, Custom::new(&log), container()),
        Err(TreeError::InvalidOrder)
    );
    assert_eq!(tree.children(root).unwrap(), &[a]);
    assert_eq!(tree.children(a).unwrap(), &[b]);
    assert_eq!(log.borrow().len(), 3);
}

#[test]
fn budgets_apply_to_subtree_migration_as_well_as_insertion() {
    let log = Log::default();
    let mut tree = Tree::new(
        Custom::new(&log),
        container(),
        TreeLimits {
            max_nodes: 4,
            max_depth: 3,
        },
    )
    .unwrap();
    let root = tree.root();
    let a = tree
        .insert(root, 0, Custom::new(&log), container())
        .unwrap();
    let b = tree.insert(a, 0, Custom::new(&log), container()).unwrap();
    let c = tree
        .insert(root, 1, Custom::new(&log), container())
        .unwrap();
    assert_eq!(tree.reparent(a, c, 0), Err(TreeError::BudgetExceeded));
    assert_eq!(
        tree.insert(b, 0, Custom::new(&log), container()),
        Err(TreeError::BudgetExceeded)
    );
    assert_eq!(tree.len(), 4);
    assert_eq!(tree.children(root).unwrap(), &[a, c]);
}

#[test]
fn paint_only_updates_skip_layout_and_geometry_updates_require_it() {
    let (mut tree, _) = setup();
    let root = tree.root();
    let mut engine = MeasureLayout::default();
    assert_eq!(tree.scene().unwrap_err(), TreeError::LayoutRequired);
    assert!(tree.layout(&mut engine, viewport()).unwrap());
    tree.scene().unwrap();
    assert_eq!(tree.invalidation(), Invalidation::None);
    tree.update_widget::<Custom>(root, Invalidation::Paint, |w| w.color = Color::BLACK)
        .unwrap();
    assert!(!tree.layout(&mut engine, viewport()).unwrap());
    let scene = tree.scene().unwrap();
    assert!(matches!(
        scene.commands()[0],
        Command::Fill {
            color: Color::BLACK,
            ..
        }
    ));
    tree.update_widget::<Custom>(root, Invalidation::Layout, |w| {
        w.size = Size::new(50.0, 20.0)
    })
    .unwrap();
    assert_eq!(tree.hit_test(Point::ZERO), Err(TreeError::LayoutRequired));
    tree.layout(&mut engine, viewport()).unwrap();
    assert_eq!(tree.layout_box(root).unwrap().bounds.size.width, 50.0);
    tree.layout(&mut engine, Size::new(400.0, 200.0)).unwrap();
    assert_eq!(engine.calls, 3);
}

struct Positioned(Vec<(NodeId, Rect)>);
impl LayoutEngine for Positioned {
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

#[test]
fn hit_order_coordinates_and_clips_match_paint_order() {
    let (mut tree, log) = setup();
    let root = tree.root();
    let a = tree
        .insert(root, 0, Custom::new(&log), LayoutStyle::default())
        .unwrap();
    let b = tree
        .insert(root, 1, Custom::new(&log), LayoutStyle::default())
        .unwrap();
    let mut engine = Positioned(vec![
        (root, Rect::new(10.0, 20.0, 20.0, 20.0)),
        (a, Rect::new(5.0, 5.0, 40.0, 40.0)),
        (b, Rect::new(5.0, 5.0, 40.0, 40.0)),
    ]);
    tree.layout(&mut engine, viewport()).unwrap();
    let hit = tree.hit_test(Point::new(17.0, 28.0)).unwrap().unwrap();
    assert_eq!(hit.id, b);
    assert_eq!(hit.local, Point::new(2.0, 3.0));
    assert_eq!(
        tree.hit_test(Point::new(40.0, 30.0)).unwrap().unwrap().id,
        b
    );
    tree.set_clip(root, true).unwrap();
    assert!(tree.hit_test(Point::new(40.0, 30.0)).unwrap().is_none());
    let scene = tree.scene().unwrap();
    assert!(matches!(
        scene.commands().first(),
        Some(Command::PushClip { .. })
    ));
    assert!(matches!(scene.commands().last(), Some(Command::PopClip)));
    if let Command::Fill { transform, .. } = scene.commands()[2] {
        assert_eq!(transform.apply(Point::ZERO), Point::new(15.0, 25.0));
    } else {
        panic!("missing child drawing");
    }
    tree.reorder(root, &[b, a]).unwrap();
    tree.layout(&mut engine, viewport()).unwrap();
    assert_eq!(
        tree.hit_test(Point::new(17.0, 28.0)).unwrap().unwrap().id,
        a
    );
}

#[test]
fn incomplete_or_failed_layout_cannot_publish_a_scene() {
    let (mut tree, log) = setup();
    let root = tree.root();
    tree.insert(root, 0, Custom::new(&log), LayoutStyle::default())
        .unwrap();
    let mut partial = Positioned(vec![(root, Rect::new(0.0, 0.0, 50.0, 50.0))]);
    assert_eq!(
        tree.layout(&mut partial, viewport()),
        Err(TreeError::IncompleteLayout)
    );
    assert_eq!(tree.scene().unwrap_err(), TreeError::LayoutRequired);
    tree.layout(&mut MeasureLayout::default(), viewport())
        .unwrap();
    assert!(tree.scene().is_ok());
    struct Failed;
    impl LayoutEngine for Failed {
        fn compute(
            &mut self,
            tree: &mut dyn LayoutTree,
            root: NodeId,
            _: Size,
        ) -> Result<(), LayoutError> {
            tree.set_layout(root, LayoutBox::new(Rect::new(0.0, 0.0, 500.0, 500.0)));
            Err(LayoutError::MeasurementFailed(root))
        }
    }
    assert!(tree.layout(&mut Failed, Size::new(1.0, 1.0)).is_err());
    assert_eq!(tree.scene().unwrap_err(), TreeError::LayoutRequired);
    tree.layout(&mut MeasureLayout::default(), viewport())
        .unwrap();
    assert_eq!(tree.layout_box(root).unwrap().bounds.size.width, 20.0);
}
