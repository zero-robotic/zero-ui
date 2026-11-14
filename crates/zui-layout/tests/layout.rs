use std::collections::BTreeMap;
use zui_api::layout::*;
use zui_layout::{LayoutLimits, TaffyLayout};
use zui_types::{Rect, Size};

#[derive(Default)]
struct Tree {
    nodes: BTreeMap<NodeId, Node>,
    writes: usize,
    measures: Vec<MeasureInput>,
    fail: bool,
    keys: BTreeMap<NodeId, LayoutCacheKey>,
    measured_ids: Vec<NodeId>,
}
struct Node {
    style: LayoutStyle,
    children: Vec<NodeId>,
    natural: Size,
    layout: LayoutBox,
}
impl Tree {
    fn add(&mut self, id: u64, style: LayoutStyle, children: &[u64], natural: Size) {
        self.nodes.insert(
            NodeId(id),
            Node {
                style,
                children: children.iter().map(|&id| NodeId(id)).collect(),
                natural,
                layout: LayoutBox::default(),
            },
        );
    }
    fn bounds(&self, id: u64) -> Rect {
        self.nodes[&NodeId(id)].layout.bounds
    }
}
impl LayoutTree for Tree {
    fn cache_key(&self, id: NodeId) -> Option<LayoutCacheKey> {
        self.keys.get(&id).cloned()
    }
    fn node(&self, id: NodeId) -> Option<LayoutNode<'_>> {
        self.nodes.get(&id).map(|n| LayoutNode {
            style: &n.style,
            children: &n.children,
        })
    }
    fn measure(&mut self, id: NodeId, input: MeasureInput) -> Result<Size, LayoutError> {
        self.measures.push(input);
        self.measured_ids.push(id);
        if self.fail {
            return Err(LayoutError::MeasurementFailed(id));
        }
        let natural = self.nodes[&id].natural;
        // A deterministic wrapping content fixture, not a font/text implementation.
        let width = input.known_width.unwrap_or(match input.available_width {
            AvailableSpace::Definite(width) => width.min(natural.width as f32),
            AvailableSpace::MinContent => (natural.width as f32).min(20.0),
            AvailableSpace::MaxContent => natural.width as f32,
        }) as f64;
        let height = input.known_height.map(f64::from).unwrap_or_else(|| {
            if width > 0.0 {
                (natural.width / width).ceil().max(1.0) * natural.height
            } else {
                natural.height
            }
        });
        Ok(Size::new(width, height))
    }
    fn set_layout(&mut self, id: NodeId, layout: LayoutBox) {
        self.writes += 1;
        self.nodes.get_mut(&id).unwrap().layout = layout;
    }
}
fn root(kind: LayoutKind) -> LayoutStyle {
    LayoutStyle {
        kind,
        width: Dimension::Percent(1.0),
        height: Dimension::Percent(1.0),
        ..Default::default()
    }
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

#[test]
fn row_padding_gap_and_growth_use_border_boxes() {
    let mut tree = Tree::default();
    tree.add(
        1,
        LayoutStyle {
            padding: Edges::all(10.0),
            gap: Size::new(10.0, 0.0),
            ..root(LayoutKind::Row)
        },
        &[2, 3],
        Size::ZERO,
    );
    tree.add(
        2,
        LayoutStyle {
            width: Dimension::Length(80.0),
            shrink: 0.0,
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    tree.add(
        3,
        LayoutStyle {
            grow: 1.0,
            min_width: Dimension::Length(0.0),
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    TaffyLayout::default()
        .compute(&mut tree, NodeId(1), Size::new(300.0, 100.0))
        .unwrap();
    assert_eq!(tree.bounds(2), Rect::new(10.0, 10.0, 80.0, 80.0));
    assert_eq!(tree.bounds(3), Rect::new(100.0, 10.0, 190.0, 80.0));
}

#[test]
fn constrained_content_is_remeasured_on_resize() {
    let mut tree = Tree::default();
    tree.add(
        1,
        LayoutStyle {
            height: Dimension::Auto,
            padding: Edges::all(10.0),
            ..root(LayoutKind::Column)
        },
        &[2],
        Size::ZERO,
    );
    tree.add(2, LayoutStyle::default(), &[], Size::new(200.0, 20.0));
    let mut engine = TaffyLayout::default();
    engine
        .compute(&mut tree, NodeId(1), Size::new(120.0, 200.0))
        .unwrap();
    close(tree.bounds(2).size.height, 40.0);
    close(tree.bounds(1).size.height, 60.0);
    assert!(
        tree.measures.iter().any(|m| m.known_width == Some(100.0)
            || m.available_width == AvailableSpace::Definite(100.0))
    );
    engine
        .compute(&mut tree, NodeId(1), Size::new(220.0, 200.0))
        .unwrap();
    close(tree.bounds(2).size.height, 20.0);
    close(tree.bounds(1).size.height, 40.0);
}

#[test]
fn grid_fractions_and_percentage_sizes() {
    let mut tree = Tree::default();
    tree.add(
        1,
        LayoutStyle {
            columns: vec![Track::Fraction(1.0), Track::Fraction(2.0)],
            gap: Size::new(10.0, 0.0),
            ..root(LayoutKind::Grid)
        },
        &[2, 3],
        Size::ZERO,
    );
    for id in [2, 3] {
        tree.add(
            id,
            LayoutStyle {
                height: Dimension::Percent(0.5),
                ..Default::default()
            },
            &[],
            Size::ZERO,
        );
    }
    TaffyLayout::default()
        .compute(&mut tree, NodeId(1), Size::new(310.0, 100.0))
        .unwrap();
    close(tree.bounds(2).size.width, 100.0);
    close(tree.bounds(3).origin.x, 110.0);
    close(tree.bounds(3).size.width, 200.0);
    close(tree.bounds(3).size.height, 50.0);
}

#[test]
fn flex_wrap_and_stable_identity_survive_reorder_and_removal() {
    let mut tree = Tree::default();
    let large_id = u64::MAX - 1;
    tree.add(
        1,
        LayoutStyle {
            wrap: true,
            align: Align::Start,
            ..root(LayoutKind::Row)
        },
        &[2, large_id],
        Size::ZERO,
    );
    for id in [2, large_id] {
        tree.add(
            id,
            LayoutStyle {
                width: Dimension::Length(70.0),
                height: Dimension::Length(20.0),
                ..Default::default()
            },
            &[],
            Size::ZERO,
        );
    }
    let mut engine = TaffyLayout::default();
    engine
        .compute(&mut tree, NodeId(1), Size::new(100.0, 40.0))
        .unwrap();
    close(tree.bounds(large_id).origin.y, 20.0);
    tree.nodes.get_mut(&NodeId(1)).unwrap().children.reverse();
    engine
        .compute(&mut tree, NodeId(1), Size::new(100.0, 40.0))
        .unwrap();
    close(tree.bounds(large_id).origin.y, 0.0);
    tree.nodes.get_mut(&NodeId(1)).unwrap().children.pop();
    tree.nodes.remove(&NodeId(2));
    engine
        .compute(&mut tree, NodeId(1), Size::new(100.0, 40.0))
        .unwrap();
    close(tree.bounds(large_id).origin.y, 0.0);
}

#[test]
fn shrink_respects_explicit_minimum_and_maximum() {
    let mut tree = Tree::default();
    tree.add(1, root(LayoutKind::Row), &[2, 3], Size::ZERO);
    for id in [2, 3] {
        tree.add(
            id,
            LayoutStyle {
                width: Dimension::Length(200.0),
                min_width: Dimension::Length(0.0),
                max_width: Dimension::Length(150.0),
                ..Default::default()
            },
            &[],
            Size::ZERO,
        );
    }
    TaffyLayout::default()
        .compute(&mut tree, NodeId(1), Size::new(200.0, 50.0))
        .unwrap();
    close(tree.bounds(2).size.width, 100.0);
    close(tree.bounds(3).size.width, 100.0);
}

#[test]
fn invalid_topology_styles_and_budgets_do_not_publish_partial_results() {
    let mut tree = Tree::default();
    tree.add(1, root(LayoutKind::Row), &[1], Size::ZERO);
    let mut engine = TaffyLayout::default();
    let viewport = Size::new(100.0, 100.0);
    assert_eq!(
        engine.compute(&mut tree, NodeId(1), viewport),
        Err(LayoutError::RepeatedNode(NodeId(1)))
    );
    tree.nodes.get_mut(&NodeId(1)).unwrap().children = vec![NodeId(2)];
    assert_eq!(
        engine.compute(&mut tree, NodeId(1), viewport),
        Err(LayoutError::MissingNode(NodeId(2)))
    );
    tree.add(
        2,
        LayoutStyle {
            width: Dimension::Length(f32::NAN),
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    assert_eq!(
        engine.compute(&mut tree, NodeId(1), viewport),
        Err(LayoutError::InvalidStyle(NodeId(2)))
    );
    tree.nodes.get_mut(&NodeId(2)).unwrap().style = LayoutStyle::default();
    let mut limited = TaffyLayout::new(LayoutLimits {
        max_depth: 1,
        ..Default::default()
    });
    assert_eq!(
        limited.compute(&mut tree, NodeId(1), viewport),
        Err(LayoutError::BudgetExceeded)
    );
    assert_eq!(tree.writes, 0);
}

#[test]
fn measurement_errors_preserve_previous_geometry() {
    let mut tree = Tree::default();
    tree.add(1, LayoutStyle::default(), &[], Size::new(20.0, 20.0));
    let mut engine = TaffyLayout::default();
    let viewport = Size::new(100.0, 100.0);
    engine.compute(&mut tree, NodeId(1), viewport).unwrap();
    let previous = tree.bounds(1);
    tree.fail = true;
    assert_eq!(
        engine.compute(&mut tree, NodeId(1), viewport),
        Err(LayoutError::MeasurementFailed(NodeId(1)))
    );
    assert_eq!(tree.bounds(1), previous);
    assert_eq!(tree.writes, 1);
}

#[test]
fn exhausted_padding_space_is_zero_at_the_public_measurement_boundary() {
    for width in [0.0, 4.0, 8.0] {
        let mut tree = Tree::default();
        tree.add(
            1,
            LayoutStyle {
                width: Dimension::Length(width),
                height: Dimension::Length(width),
                padding: Edges::all(8.0),
                ..Default::default()
            },
            &[],
            Size::new(200.0, 20.0),
        );
        TaffyLayout::default()
            .compute(&mut tree, NodeId(1), Size::new(width as f64, width as f64))
            .unwrap();
        assert!(!tree.measures.is_empty());
        for input in &tree.measures {
            for known in [input.known_width, input.known_height]
                .into_iter()
                .flatten()
            {
                assert!(known.is_finite() && known >= 0.0);
            }
            for available in [input.available_width, input.available_height] {
                if let AvailableSpace::Definite(value) = available {
                    assert!(value.is_finite() && value >= 0.0);
                }
            }
        }
        assert!(tree.bounds(1).is_valid());
    }
}

#[test]
fn stack_shares_content_area_and_honors_per_child_alignment_and_margins() {
    let mut tree = Tree::default();
    tree.add(
        1,
        LayoutStyle {
            kind: LayoutKind::Stack,
            width: Dimension::Length(200.0),
            height: Dimension::Length(120.0),
            padding: Edges::all(10.0),
            ..Default::default()
        },
        &[2, 3, 4],
        Size::ZERO,
    );
    tree.add(2, LayoutStyle::default(), &[], Size::new(80.0, 20.0));
    tree.add(
        3,
        LayoutStyle {
            width: Dimension::Length(60.0),
            height: Dimension::Length(30.0),
            align_self: Some(Align::Center),
            justify_self: Some(Align::Center),
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    tree.add(
        4,
        LayoutStyle {
            width: Dimension::Length(20.0),
            height: Dimension::Length(10.0),
            align_self: Some(Align::Start),
            justify_self: Some(Align::End),
            margin: Edges {
                right: 5.0,
                top: 7.0,
                ..Default::default()
            },
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    TaffyLayout::default()
        .compute(&mut tree, NodeId(1), Size::new(300.0, 200.0))
        .unwrap();
    assert_eq!(tree.bounds(2), Rect::new(10.0, 10.0, 180.0, 100.0));
    assert_eq!(tree.bounds(3), Rect::new(70.0, 45.0, 60.0, 30.0));
    assert_eq!(tree.bounds(4), Rect::new(165.0, 17.0, 20.0, 10.0));
}
#[test]
fn stack_intrinsic_size_uses_maximum_layer_extent_and_nested_rows_keep_flow_layout() {
    let mut tree = Tree::default();
    tree.add(
        1,
        LayoutStyle {
            kind: LayoutKind::Row,
            width: Dimension::Length(300.0),
            align: Align::Start,
            ..Default::default()
        },
        &[2],
        Size::ZERO,
    );
    tree.add(
        2,
        LayoutStyle {
            kind: LayoutKind::Stack,
            padding: Edges::all(6.0),
            ..Default::default()
        },
        &[3, 4],
        Size::ZERO,
    );
    tree.add(
        3,
        LayoutStyle {
            width: Dimension::Length(80.0),
            height: Dimension::Length(20.0),
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    tree.add(
        4,
        LayoutStyle {
            width: Dimension::Length(30.0),
            height: Dimension::Length(50.0),
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    let mut engine = TaffyLayout::default();
    engine
        .compute(&mut tree, NodeId(1), Size::new(300.0, 200.0))
        .unwrap();
    assert_eq!(tree.bounds(2).size, Size::new(92.0, 62.0));
    tree.nodes.get_mut(&NodeId(2)).unwrap().children.reverse();
    engine
        .compute(&mut tree, NodeId(1), Size::new(300.0, 200.0))
        .unwrap();
    assert_eq!(tree.bounds(2).size, Size::new(92.0, 62.0));
    tree.add(
        1,
        LayoutStyle {
            kind: LayoutKind::Stack,
            width: Dimension::Length(200.0),
            height: Dimension::Length(100.0),
            ..Default::default()
        },
        &[2],
        Size::ZERO,
    );
    tree.add(
        2,
        LayoutStyle {
            kind: LayoutKind::Row,
            width: Dimension::Percent(0.5),
            height: Dimension::Percent(0.5),
            align_self: Some(Align::End),
            justify_self: Some(Align::End),
            ..Default::default()
        },
        &[3, 4],
        Size::ZERO,
    );
    tree.add(
        3,
        LayoutStyle {
            width: Dimension::Length(20.0),
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    tree.add(
        4,
        LayoutStyle {
            width: Dimension::Length(30.0),
            ..Default::default()
        },
        &[],
        Size::ZERO,
    );
    engine
        .compute(&mut tree, NodeId(1), Size::new(300.0, 200.0))
        .unwrap();
    assert_eq!(tree.bounds(2), Rect::new(100.0, 50.0, 100.0, 50.0));
    assert_eq!(tree.bounds(3).origin.x, 0.0);
    assert_eq!(tree.bounds(4).origin.x, 20.0);
}
#[test]
fn stack_rejects_gap_tracks_and_wrap_and_respects_track_budget() {
    let mut tree = Tree::default();
    let base = LayoutStyle {
        kind: LayoutKind::Stack,
        ..Default::default()
    };
    let mut styles = Vec::new();
    let mut style = base.clone();
    style.gap = Size::new(1.0, 0.0);
    styles.push(style);
    let mut style = base.clone();
    style.columns = vec![Track::Auto];
    styles.push(style);
    let mut style = base.clone();
    style.wrap = true;
    styles.push(style);
    for style in styles {
        tree.add(1, style, &[], Size::ZERO);
        assert_eq!(
            TaffyLayout::default().compute(&mut tree, NodeId(1), Size::new(100.0, 100.0)),
            Err(LayoutError::InvalidStyle(NodeId(1)))
        );
        assert_eq!(tree.writes, 0);
    }
    tree.add(1, base, &[], Size::ZERO);
    assert_eq!(
        TaffyLayout::new(LayoutLimits {
            max_tracks_per_axis: 0,
            ..Default::default()
        })
        .compute(&mut tree, NodeId(1), Size::new(100.0, 100.0)),
        Err(LayoutError::BudgetExceeded)
    );
}

#[test]
fn intrinsic_width_keywords_preserve_natural_size_or_soft_wrap_width() {
    for (dimension, width, height) in [
        (Dimension::MaxContent, 200.0, 20.0),
        (Dimension::MinContent, 20.0, 200.0),
    ] {
        let mut tree = Tree::default();
        tree.add(
            1,
            LayoutStyle {
                align: Align::Start,
                columns: vec![Track::Auto],
                ..root(LayoutKind::Grid)
            },
            &[2],
            Size::ZERO,
        );
        tree.add(
            2,
            LayoutStyle {
                width: dimension,
                shrink: 0.0,
                ..Default::default()
            },
            &[],
            Size::new(200.0, 20.0),
        );
        TaffyLayout::default()
            .compute(&mut tree, NodeId(1), Size::new(60.0, 300.0))
            .unwrap();
        close(tree.bounds(2).size.width, width);
        close(tree.bounds(2).size.height, height);
    }
}
#[test]
fn unsupported_intrinsic_min_max_bounds_are_rejected_without_publishing() {
    for field in 0..4 {
        let mut style = root(LayoutKind::Column);
        match field {
            0 => style.min_width = Dimension::MaxContent,
            1 => style.max_width = Dimension::MinContent,
            2 => style.min_height = Dimension::MinContent,
            _ => style.max_height = Dimension::MaxContent,
        }
        let mut tree = Tree::default();
        tree.add(1, style, &[], Size::ZERO);
        assert_eq!(
            TaffyLayout::default().compute(&mut tree, NodeId(1), Size::new(100.0, 100.0)),
            Err(LayoutError::InvalidStyle(NodeId(1)))
        );
        assert_eq!(tree.writes, 0);
    }
}

fn cached_fixture() -> Tree {
    let mut tree = Tree::default();
    tree.add(1, root(LayoutKind::Column), &[2, 3], Size::ZERO);
    tree.add(2, LayoutStyle::default(), &[], Size::new(50.0, 10.0));
    tree.add(3, LayoutStyle::default(), &[], Size::new(70.0, 15.0));
    tree.keys = tree
        .nodes
        .keys()
        .map(|&id| (id, LayoutCacheKey::default()))
        .collect();
    tree
}
fn matches_fresh(tree: &mut Tree, engine: &mut TaffyLayout, size: Size) {
    engine.compute(tree, NodeId(1), size).unwrap();
    let cached: Vec<_> = tree.nodes.values().map(|n| n.layout).collect();
    TaffyLayout::default()
        .compute(tree, NodeId(1), size)
        .unwrap();
    assert_eq!(
        cached,
        tree.nodes.values().map(|n| n.layout).collect::<Vec<_>>()
    );
}
#[test]
fn cross_pass_cache_reuses_siblings_and_invalidates_changed_measurement() {
    let mut tree = cached_fixture();
    let mut engine = TaffyLayout::default();
    let size = Size::new(200.0, 200.0);
    engine.compute(&mut tree, NodeId(1), size).unwrap();
    tree.measured_ids.clear();
    engine.compute(&mut tree, NodeId(1), size).unwrap();
    assert!(tree.measured_ids.is_empty());
    tree.nodes.get_mut(&NodeId(2)).unwrap().natural.height = 30.0;
    tree.keys.insert(NodeId(2), LayoutCacheKey::default());
    engine.compute(&mut tree, NodeId(1), size).unwrap();
    assert!(tree.measured_ids.contains(&NodeId(2)));
    assert!(!tree.measured_ids.contains(&NodeId(3)));
    matches_fresh(&mut tree, &mut engine, size);
    matches_fresh(&mut tree, &mut engine, Size::new(35.0, 100.0));
    tree.nodes.get_mut(&NodeId(1)).unwrap().children.reverse();
    matches_fresh(&mut tree, &mut engine, size);
    tree.nodes.get_mut(&NodeId(3)).unwrap().style.padding = Edges::all(4.0);
    matches_fresh(&mut tree, &mut engine, size);
    tree.nodes
        .get_mut(&NodeId(1))
        .unwrap()
        .children
        .retain(|id| *id != NodeId(2));
    tree.nodes.remove(&NodeId(2));
    matches_fresh(&mut tree, &mut engine, size);
}
#[test]
fn cache_isolates_owners_and_recovers_from_failed_passes() {
    let mut engine = TaffyLayout::default();
    let size = Size::new(200.0, 200.0);
    let mut first = cached_fixture();
    engine.compute(&mut first, NodeId(1), size).unwrap();
    let mut second = cached_fixture();
    second.nodes.get_mut(&NodeId(2)).unwrap().natural.height = 45.0;
    matches_fresh(&mut second, &mut engine, size);
    second.fail = true;
    second.keys.insert(NodeId(2), LayoutCacheKey::default());
    let writes = second.writes;
    assert!(engine.compute(&mut second, NodeId(1), size).is_err());
    assert_eq!(second.writes, writes);
    second.fail = false;
    matches_fresh(&mut second, &mut engine, size);
    second.keys.clear();
    engine.compute(&mut second, NodeId(1), size).unwrap();
    second.measured_ids.clear();
    engine.compute(&mut second, NodeId(1), size).unwrap();
    assert!(
        !second.measured_ids.is_empty(),
        "legacy owners remain uncached"
    );
}

#[test]
fn cached_flex_grid_and_stack_match_fresh_layout_after_reorder_and_resize() {
    for kind in [LayoutKind::Row, LayoutKind::Grid, LayoutKind::Stack] {
        let mut tree = cached_fixture();
        let style = &mut tree.nodes.get_mut(&NodeId(1)).unwrap().style;
        style.kind = kind;
        if kind == LayoutKind::Grid {
            style.columns = vec![Track::Fraction(1.0), Track::Fraction(2.0)];
        }
        let mut engine = TaffyLayout::default();
        for size in [
            Size::new(200.0, 100.0),
            Size::new(40.0, 80.0),
            Size::new(200.0, 100.0),
        ] {
            matches_fresh(&mut tree, &mut engine, size);
            matches_fresh(&mut tree, &mut engine, size);
            tree.nodes.get_mut(&NodeId(1)).unwrap().children.reverse();
            matches_fresh(&mut tree, &mut engine, size);
            tree.nodes.get_mut(&NodeId(2)).unwrap().natural.height += 10.0;
            tree.keys.insert(NodeId(2), LayoutCacheKey::default());
            matches_fresh(&mut tree, &mut engine, size);
        }
    }
}
