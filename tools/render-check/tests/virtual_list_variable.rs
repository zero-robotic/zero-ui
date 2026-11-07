use std::rc::Rc;
use zui_api::{
    layout::{Dimension, Edges, LayoutError, LayoutStyle},
    view::{Key, View},
    widget::Invalidation,
};
use zui_core::{Tree, TreeError};
use zui_custom_widget_example::MeterView;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{ScrollAlignment, VirtualList, VirtualListView};

fn color(i: usize) -> Color {
    if i.is_multiple_of(2) {
        Color::rgb(200, 0, 0)
    } else {
        Color::rgb(0, 180, 0)
    }
}
fn view(heights: Rc<[f64]>) -> VirtualListView {
    VirtualListView::variable(heights, |i| {
        View::new(MeterView::new(1.0, color(i), color(i)).unwrap())
            .keyed(i as u64)
            .styled(LayoutStyle {
                height: Dimension::Percent(1.0),
                ..Default::default()
            })
    })
    .keyed("list")
    .gap(2.0)
    .scrollbar(None)
    .style(|s| {
        s.width = Dimension::Length(100.0);
        s.height = Dimension::Length(80.0);
        s.padding = Edges::all(10.0);
    })
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(100.0, 80.0),
    )
    .unwrap();
}
fn offset(tree: &mut Tree) -> f64 {
    let mut result = 0.0;
    tree.update_widget::<VirtualList>(tree.root(), Invalidation::None, |list| {
        result = list.offset()
    })
    .unwrap();
    result
}
fn mixed() -> Rc<[f64]> {
    (0..4000)
        .map(|i| [12.0, 28.0, 16.0, 44.0][i % 4])
        .collect::<Vec<_>>()
        .into()
}
#[test]
fn mixed_sizes_position_slots_and_keep_index_jumps_local() {
    let heights = mixed();
    let mut tree = Tree::from_view(
        view(heights.clone())
            .scroll_to_index(3000, ScrollAlignment::Start, 1)
            .build(),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(offset(&mut tree), 81_000.0);
    assert!(tree.len() <= 21);
    let slots = tree.children(tree.root()).unwrap();
    for &slot in slots {
        let Key::Number(index) = tree.key(slot).unwrap().unwrap() else {
            panic!()
        };
        let bounds = tree.layout_box(slot).unwrap().bounds;
        assert_eq!(bounds.size.height, heights[*index as usize]);
        assert!(bounds.origin.y < 300.0);
    }
    for (y, index) in [(11.0, 3000), (25.0, 3001), (55.0, 3002)] {
        let hit = tree.hit_test(Point::new(15.0, y)).unwrap().unwrap().id;
        assert_eq!(tree.key(hit).unwrap(), Some(&Key::Number(index)));
    }
    let old = tree.children(tree.root()).unwrap().to_vec();
    tree.reconcile(
        view(heights)
            .scroll_to_index(3000, ScrollAlignment::Start, 1)
            .build(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(tree.children(tree.root()).unwrap(), old);
}
#[test]
fn changed_heights_optionally_preserve_anchor_and_new_requests_take_priority() {
    let old: Rc<[f64]> = vec![20.0; 20].into();
    let mut tree = Tree::from_view(
        view(old.clone())
            .scroll_to(49.0, 1)
            .preserve_visible_anchor(true)
            .build(),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree);
    let hit = tree.hit_test(Point::new(15.0, 11.0)).unwrap().unwrap().id;
    assert_eq!(tree.key(hit).unwrap(), Some(&Key::Number(2)));
    let mut changed = old.to_vec();
    changed[0] = 50.0;
    let changed: Rc<[f64]> = changed.into();
    tree.reconcile(
        view(changed.clone())
            .scroll_to(49.0, 1)
            .preserve_visible_anchor(true)
            .build(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(offset(&mut tree), 79.0);
    assert_eq!(
        tree.hit_test(Point::new(15.0, 11.0)).unwrap().unwrap().id,
        hit
    );
    tree.reconcile(
        view(old.clone())
            .scroll_to(10.0, 2)
            .preserve_visible_anchor(true)
            .build(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(offset(&mut tree), 10.0);
    tree.reconcile(
        view(changed)
            .scroll_to_index(4, ScrollAlignment::Start, 3)
            .preserve_visible_anchor(true)
            .build(),
    )
    .unwrap();
    layout(&mut tree);
    assert_eq!(offset(&mut tree), 118.0);
    tree.reconcile(view(old).preserve_visible_anchor(false).build())
        .unwrap();
    layout(&mut tree);
    assert_eq!(offset(&mut tree), 118.0); // pixel offset policy is the default
}
#[test]
fn invalid_height_update_is_retryable_and_empty_variable_data_is_supported() {
    let valid: Rc<[f64]> = vec![20.0; 10].into();
    let mut tree = Tree::from_view(view(valid.clone()).build(), Default::default()).unwrap();
    layout(&mut tree);
    let rows = tree.children(tree.root()).unwrap().to_vec();
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-100] {
        let mut values = valid.to_vec();
        values[5] = invalid;
        tree.reconcile(view(values.into()).build()).unwrap();
        assert!(matches!(
            tree.layout(
                &mut zui_layout::TaffyLayout::default(),
                Size::new(100.0, 80.0)
            ),
            Err(TreeError::Layout(LayoutError::InvalidStyle(_)))
        ));
        assert_eq!(tree.children(tree.root()).unwrap(), rows);
    }
    tree.reconcile(view(valid).build()).unwrap();
    layout(&mut tree);
    assert_eq!(tree.children(tree.root()).unwrap(), rows);
    tree.reconcile(view(Rc::from([])).build()).unwrap();
    layout(&mut tree);
    assert_eq!(tree.len(), 1);
}

fn verify(renderer: &mut impl OffscreenRenderer) {
    let heights = mixed();
    let mut starts = Vec::with_capacity(heights.len());
    let mut cursor = 0.0;
    for &height in heights.iter() {
        starts.push(cursor);
        cursor += height + 2.0;
    }
    for offset in [0.0, 81_005.0, 107_938.0] {
        let mut tree = Tree::from_view(
            view(heights.clone()).scroll_to(offset, 1).build(),
            Default::default(),
        )
        .unwrap();
        layout(&mut tree);
        for scale in [1, 2] {
            let pixels = renderer
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(100 * scale, 80 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for y in 0..80 * scale as usize {
                for x in 0..100 * scale as usize {
                    let px = x / scale as usize;
                    let py = y / scale as usize;
                    let logical = py as f64 - 10.0 + offset;
                    let index = starts
                        .partition_point(|start| *start <= logical)
                        .saturating_sub(1);
                    let inside = (10..90).contains(&px)
                        && (10..70).contains(&py)
                        && logical >= starts[index]
                        && logical < starts[index] + heights[index];
                    let color = if inside { color(index) } else { Color::BLACK };
                    let i = y * pixels.stride() + x * 4;
                    assert_eq!(
                        &pixels.pixels()[i..i + 4],
                        &[color.r, color.g, color.b, 255],
                        "offset={offset}, pixel=({x},{y})"
                    );
                }
            }
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn mixed_rows_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn mixed_rows_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
