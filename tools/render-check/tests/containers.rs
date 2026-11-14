use custom_widget::MeterView;
use zui_api::{
    layout::{Align, Dimension, Edges, Justify, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::{Tree, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Frame, OffscreenRenderer};
use zui_types::{Color, Extent, Point, Rect, Size};
use zui_widgets::{Container, column, row};

fn child(width: f32, height: f32) -> View {
    View::new(MeterView::new(1.0, Color::WHITE, Color::BLACK).unwrap())
        .keyed("child")
        .styled(LayoutStyle {
            width: Dimension::Length(width),
            height: Dimension::Length(height),
            min_width: Dimension::Length(width),
            ..Default::default()
        })
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let viewport = Size::new(100.0, 80.0);
    let mut engine = TaffyLayout::default();
    let view = Container::new(child(20.0, 10.0))
        .background(Color::rgb(20, 40, 60))
        .style(|style| {
            style.width = Dimension::Percent(1.0);
            style.height = Dimension::Percent(1.0);
            style.padding = Edges::all(10.0);
            style.align = Align::Center;
            style.justify = Justify::Center;
        })
        .build();
    let mut tree = Tree::from_view(view, TreeLimits::default()).unwrap();
    tree.layout(&mut engine, viewport).unwrap();
    let id = tree.children(tree.root()).unwrap()[0];
    assert_eq!(
        tree.layout_box(id).unwrap().bounds,
        Rect::new(40.0, 35.0, 20.0, 10.0)
    );
    assert_eq!(
        tree.hit_test(Point::new(45.0, 40.0)).unwrap().unwrap().id,
        id
    );
    assert!(tree.hit_test(Point::new(5.0, 5.0)).unwrap().is_none());
    let scene = tree.scene().unwrap();
    for scale in [1.0, 2.0] {
        let frame = Frame {
            extent: Extent::new((120.0 * scale) as u32, (80.0 * scale) as u32),
            scale,
            background: Color::BLACK,
        };
        let pixels = renderer.render(&scene, frame).unwrap();
        for y in 0..80 * scale as usize {
            for x in 0..120 * scale as usize {
                let offset = y * pixels.stride() + x * 4;
                let expected = if (40 * scale as usize..60 * scale as usize).contains(&x)
                    && (35 * scale as usize..45 * scale as usize).contains(&y)
                {
                    [255, 255, 255, 255]
                } else if x < 100 * scale as usize {
                    [20, 40, 60, 255]
                } else {
                    [0, 0, 0, 255]
                };
                assert_eq!(&pixels.pixels()[offset..offset + 4], &expected);
            }
        }
    }
    for clip in [false, true] {
        let view = Container::new(child(120.0, 20.0))
            .clipped(clip)
            .style(|style| {
                style.width = Dimension::Length(100.0);
                style.padding = Edges::all(10.0);
            })
            .build();
        tree.reconcile(view).unwrap();
        tree.layout(&mut engine, viewport).unwrap();
        assert_eq!(
            tree.hit_test(Point::new(110.0, 15.0)).unwrap().is_some(),
            !clip
        );
        let frame = Frame {
            extent: Extent::new(140, 80),
            scale: 1.0,
            background: Color::BLACK,
        };
        let pixels = renderer.render(&tree.scene().unwrap(), frame).unwrap();
        let offset = 15 * pixels.stride() + 110 * 4;
        assert_eq!(
            &pixels.pixels()[offset..offset + 4],
            if clip {
                &[0, 0, 0, 255]
            } else {
                &[255, 255, 255, 255]
            }
        );
    }
}
#[cfg(feature = "cpu")]
#[test]
fn container_alignment_background_clipping_and_hits_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn container_alignment_background_clipping_and_hits_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
#[test]
fn container_updates_reuse_keyed_children_and_report_precise_invalidation() {
    let description = |vertical: bool, color, reverse: bool| {
        let children = if reverse { ["b", "a"] } else { ["a", "b"] }.map(|key| {
            Container::empty()
                .keyed(key)
                .background(color)
                .style(|s| {
                    s.width = Dimension::Length(20.0);
                    s.height = Dimension::Length(10.0);
                })
                .build()
        });
        (if vertical {
            column(children)
        } else {
            row(children)
        })
        .style(|s| {
            s.gap = Size::new(5.0, 5.0);
            s.align = Align::Start;
        })
        .build()
    };
    let mut tree = Tree::from_view(
        description(false, Color::WHITE, false),
        TreeLimits::default(),
    )
    .unwrap();
    let mut engine = TaffyLayout::default();
    let viewport = Size::new(100.0, 80.0);
    tree.layout(&mut engine, viewport).unwrap();
    let ids = tree.children(tree.root()).unwrap().to_vec();
    assert_eq!(
        tree.layout_box(ids[1]).unwrap().bounds.origin,
        Point::new(25.0, 0.0)
    );
    tree.scene().unwrap();
    let report = tree
        .reconcile(description(false, Color::BLACK, false))
        .unwrap();
    assert_eq!(report.invalidation, Invalidation::Paint);
    assert!(!tree.layout(&mut engine, viewport).unwrap());
    let report = tree
        .reconcile(description(true, Color::BLACK, true))
        .unwrap();
    assert_eq!(report.invalidation, Invalidation::Layout);
    assert_eq!((report.created, report.removed), (0, 0));
    assert_eq!(tree.children(tree.root()).unwrap(), &[ids[1], ids[0]]);
    tree.layout(&mut engine, viewport).unwrap();
    assert_eq!(
        tree.layout_box(ids[0]).unwrap().bounds.origin,
        Point::new(0.0, 15.0)
    );
}
