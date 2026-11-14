use custom_widget::MeterView;
use std::{cell::Cell, rc::Rc, sync::Arc};
use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::{Key as ViewKey, View},
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{Container, ScrollAlignment, TextView, VirtualList, VirtualListView};

fn height(i: usize) -> f32 {
    [12.0, 28.0, 16.0, 44.0][i % 4]
}
fn color(i: usize) -> Color {
    if i.is_multiple_of(2) {
        Color::rgb(200, 0, 0)
    } else {
        Color::rgb(0, 180, 0)
    }
}
fn view(target: usize, revision: u64, calls: Rc<Cell<usize>>) -> View {
    VirtualListView::measured(4000, 32.0, revision, move |i| {
        calls.set(calls.get() + 1);
        View::new(MeterView::new(1.0, color(i), color(i)).unwrap())
            .keyed(i as u64)
            .styled(LayoutStyle {
                height: Dimension::Length(height(i) * revision as f32),
                ..Default::default()
            })
    })
    .gap(2.0)
    .scrollbar(None)
    .scroll_to_index(target, ScrollAlignment::Start, 1)
    .style(|s| {
        s.width = Dimension::Length(100.0);
        s.height = Dimension::Length(80.0);
        s.padding = Edges::all(10.0);
    })
    .build()
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(100.0, 80.0),
    )
    .unwrap();
}
fn state(tree: &mut Tree) -> (f64, f64) {
    let mut result = (0.0, 0.0);
    tree.update_widget::<VirtualList>(tree.root(), Invalidation::None, |list| {
        result = (list.offset(), list.max_offset())
    })
    .unwrap();
    result
}
#[test]
fn estimated_rows_are_measured_without_realizing_the_full_dataset() {
    let calls = Rc::new(Cell::new(0));
    let mut tree = Tree::from_view(view(3000, 1, calls.clone()), Default::default()).unwrap();
    layout(&mut tree);
    assert!(tree.len() < 25);
    assert!(calls.get() < 60);
    let root = tree.root();
    for &slot in tree.children(root).unwrap() {
        let ViewKey::Number(i) = tree.key(slot).unwrap().unwrap() else {
            panic!()
        };
        let bounds = tree.layout_box(slot).unwrap().bounds;
        assert_eq!(bounds.size.height, height(*i as usize) as f64);
        assert!(bounds.origin.y < 300.0);
    }
    let target = tree.hit_test(Point::new(15.0, 11.0)).unwrap().unwrap().id;
    assert_eq!(tree.key(target).unwrap(), Some(&ViewKey::Number(3000)));
    let old_state = state(&mut tree);
    tree.reconcile(view(3000, 1, calls.clone())).unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree), old_state);
    assert_eq!(
        tree.hit_test(Point::new(15.0, 11.0)).unwrap().unwrap().id,
        target
    );
    tree.reconcile(view(3000, 2, calls)).unwrap();
    layout(&mut tree);
    assert_eq!(
        tree.hit_test(Point::new(15.0, 11.0)).unwrap().unwrap().id,
        target
    );
    for &slot in tree.children(root).unwrap() {
        let ViewKey::Number(i) = tree.key(slot).unwrap().unwrap() else {
            panic!()
        };
        assert_eq!(
            tree.layout_box(slot).unwrap().bounds.size.height,
            height(*i as usize) as f64 * 2.0
        );
    }
}
#[test]
fn actual_text_reflows_after_width_and_font_epoch_changes() {
    let mut text = zui_text::ParleyText::default();
    let font: Arc<[u8]> =
        Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice());
    let families = text.register_font(font.clone()).unwrap();
    let view = VirtualListView::measured(1000, 40.0, 1, move |i| {
        View::new(TextView::themed(
            &zui_theme::Theme::dark(),
            format!("记录 {i}：自动行高使用实际文字换行结果，视口宽度改变时会重新测量。"),
            families.clone(),
            zui_theme::TypeRole::Body,
        ))
        .keyed(i as u64)
    })
    .scrollbar(None)
    .scroll_to_index(200, ScrollAlignment::Start, 1)
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Length(120.0);
    })
    .build();
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    let mut engine = zui_layout::TaffyLayout::default();
    tree.layout_with_text(&mut engine, &mut text, Size::new(300.0, 120.0))
        .unwrap();
    let slot = tree
        .children(tree.root())
        .unwrap()
        .iter()
        .copied()
        .find(|id| tree.key(*id).unwrap() == Some(&ViewKey::Number(200)))
        .unwrap();
    let old_height = tree.layout_box(slot).unwrap().bounds.size.height;
    let target = tree.hit_test(Point::new(5.0, 5.0)).unwrap().unwrap().id;
    assert_eq!(tree.key(target).unwrap(), Some(&ViewKey::Number(200)));
    tree.layout_with_text(&mut engine, &mut text, Size::new(120.0, 120.0))
        .unwrap();
    assert!(tree.layout_box(slot).unwrap().bounds.size.height > old_height);
    assert_eq!(
        tree.hit_test(Point::new(5.0, 5.0)).unwrap().unwrap().id,
        target
    );
    let narrow = state(&mut tree);
    let epoch = zui_api::text::TextService::font_epoch(&text);
    text.register_font(font).unwrap();
    assert!(zui_api::text::TextService::font_epoch(&text) > epoch);
    tree.layout_with_text(&mut engine, &mut text, Size::new(120.0, 120.0))
        .unwrap();
    assert_eq!(
        tree.hit_test(Point::new(5.0, 5.0)).unwrap().unwrap().id,
        target
    );
    assert_eq!(state(&mut tree), narrow);
}
#[test]
fn measured_target_alignment_and_empty_rows_are_bounded() {
    for alignment in [ScrollAlignment::Center, ScrollAlignment::End] {
        let view = VirtualListView::measured(100, 10.0, 1, |i| {
            View::new(MeterView::new(1.0, color(i), color(i)).unwrap())
                .keyed(i as u64)
                .styled(LayoutStyle {
                    height: Dimension::Length(30.0),
                    ..Default::default()
                })
        })
        .overscan(2)
        .scrollbar(None)
        .scroll_to_index(50, alignment, 1)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(100.0);
        })
        .build();
        let mut tree = Tree::from_view(view, Default::default()).unwrap();
        layout(&mut tree);
        let y = if alignment == ScrollAlignment::Center {
            36.0
        } else {
            71.0
        };
        let slot = tree.hit_test(Point::new(5.0, y)).unwrap().unwrap().id;
        assert_eq!(tree.key(slot).unwrap(), Some(&ViewKey::Number(50)));
    }
    let view =
        VirtualListView::measured(10, 1.0, 1, |i| Container::empty().keyed(i as u64).build())
            .scrollbar(None)
            .style(|s| {
                s.height = Dimension::Length(10.0);
                s.width = Dimension::Length(100.0);
            })
            .build();
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    layout(&mut tree);
    assert_eq!(tree.children(tree.root()).unwrap().len(), 10);
    for &id in tree.children(tree.root()).unwrap() {
        assert_eq!(tree.layout_box(id).unwrap().bounds.size.height, 1.0);
    }
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    for target in [0, 3000] {
        for scroll in [0.0, 7.0] {
            let mut tree =
                Tree::from_view(view(target, 1, Rc::default()), Default::default()).unwrap();
            layout(&mut tree);
            if scroll > 0.0 {
                tree.dispatch_input(InputEvent::PointerMoved(Point::new(15.0, 15.0)))
                    .unwrap();
                tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels {
                    x: 0.0,
                    y: -scroll,
                }))
                .unwrap();
                layout(&mut tree);
            }
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
                        let mut local = py as f64 - 10.0 + scroll;
                        let mut index = target;
                        while local >= height(index) as f64 + 2.0 {
                            local -= height(index) as f64 + 2.0;
                            index += 1;
                        }
                        let inside = (10..90).contains(&px)
                            && (10..70).contains(&py)
                            && local >= 0.0
                            && local < height(index) as f64;
                        let c = if inside { color(index) } else { Color::BLACK };
                        let i = y * pixels.stride() + x * 4;
                        assert_eq!(
                            &pixels.pixels()[i..i + 4],
                            &[c.r, c.g, c.b, 255],
                            "target={target},scroll={scroll},pixel=({x},{y})"
                        );
                    }
                }
            }
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn measured_rows_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn measured_rows_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

#[test]
fn rejected_measurement_blocks_drawing_and_recovers_with_new_data_revision() {
    let make = |height, revision| {
        VirtualListView::measured(1, 20.0, revision, move |_| {
            View::new(MeterView::new(1.0, Color::WHITE, Color::WHITE).unwrap())
                .keyed("row")
                .styled(LayoutStyle {
                    height: Dimension::Length(height),
                    ..Default::default()
                })
        })
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(80.0);
        })
        .build()
    };
    let mut tree = Tree::from_view(make(1e13, 1), Default::default()).unwrap();
    assert!(matches!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(100.0, 80.0)
        ),
        Err(zui_core::TreeError::Layout(
            zui_api::layout::LayoutError::InvalidMeasurement(_)
        ))
    ));
    assert!(matches!(
        tree.scene(),
        Err(zui_core::TreeError::LayoutRequired)
    ));
    tree.reconcile(make(20.0, 2)).unwrap();
    layout(&mut tree);
    assert_eq!(state(&mut tree), (0.0, 0.0));
    assert!(tree.scene().is_ok());
}

#[test]
fn million_measured_rows_use_sparse_budget_and_recover_after_budget_rejection() {
    let make = |height: f32, revision, budget| {
        VirtualListView::measured(1_000_000, 20.0, revision, move |i| {
            View::new(MeterView::new(1.0, Color::WHITE, Color::WHITE).unwrap())
                .keyed(i as u64)
                .styled(LayoutStyle {
                    height: Dimension::Length(height),
                    ..Default::default()
                })
        })
        .index_limits(zui_widgets::VirtualIndexLimits {
            max_index_bytes: budget,
            ..Default::default()
        })
        .scroll_to_index(900_000, ScrollAlignment::Start, 1)
        .scrollbar(None)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(80.0);
        })
        .build()
    };
    let bytes = |tree: &mut Tree| {
        let mut result = 0;
        tree.update_widget::<VirtualList>(tree.root(), Invalidation::None, |list| {
            result = list.allocated_index_bytes()
        })
        .unwrap();
        result
    };
    let mut tree = Tree::from_view(make(20.0, 1, 0), Default::default()).unwrap();
    layout(&mut tree);
    assert_eq!(bytes(&mut tree), 0); // All observed values match the implicit estimate.
    assert!(tree.len() < 25);
    tree.reconcile(make(30.0, 2, 0)).unwrap();
    assert!(matches!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(100.0, 80.0)
        ),
        Err(zui_core::TreeError::Layout(
            zui_api::layout::LayoutError::InvalidMeasurement(_)
        ))
    ));
    assert_eq!(bytes(&mut tree), 0);
    tree.reconcile(make(30.0, 2, 16 * 1024)).unwrap();
    layout(&mut tree);
    assert!(bytes(&mut tree) > 0 && bytes(&mut tree) <= 16 * 1024);
    assert!(tree.len() < 25);
    let target = tree.hit_test(Point::new(5.0, 5.0)).unwrap().unwrap().id;
    assert_eq!(tree.key(target).unwrap(), Some(&ViewKey::Number(900_000)));
}

#[test]
fn explicit_index_compaction_preserves_realized_identity_and_scroll_position() {
    let mut tree = Tree::from_view(view(3000, 1, Rc::default()), Default::default()).unwrap();
    layout(&mut tree);
    let root = tree.root();
    let rows = tree.children(root).unwrap().to_vec();
    let before = state(&mut tree);
    let hit = tree.hit_test(Point::new(15.0, 11.0)).unwrap().unwrap().id;
    let mut result = Ok(0);
    let mut before_bytes = 0;
    let mut after_bytes = 0;
    tree.update_widget::<VirtualList>(root, Invalidation::Layout, |list| {
        before_bytes = list.allocated_index_bytes();
        result = list.compact_index(16 * 1024);
        after_bytes = list.allocated_index_bytes();
    })
    .unwrap();
    assert_eq!(result.unwrap(), before_bytes - after_bytes);
    assert!(after_bytes <= before_bytes);
    layout(&mut tree);
    assert_eq!(tree.children(root).unwrap(), rows);
    assert_eq!(state(&mut tree), before);
    assert_eq!(
        tree.hit_test(Point::new(15.0, 11.0)).unwrap().unwrap().id,
        hit
    );
    assert!(tree.scene().is_ok());
}
