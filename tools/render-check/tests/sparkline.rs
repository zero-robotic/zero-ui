use zui_api::{
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_charts::{ChartRange, SparklineSeries, SparklineView};
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Size};
fn description(view: &SparklineView) -> View {
    View::new(view.clone()).styled(LayoutStyle {
        width: Dimension::Length(120.0),
        height: Dimension::Length(60.0),
        padding: Edges::all(10.0),
        ..Default::default()
    })
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut view = SparklineView::themed(
        &Theme::dashboard(),
        SparklineSeries::new(vec![Some(0.5), Some(0.5), None, Some(0.5), Some(0.5)]).unwrap(),
    );
    view.range = Some(ChartRange::new(0.0, 1.0).unwrap());
    view.style.line = Color::rgb(255, 0, 0);
    view.style.area = None;
    let mut tree = zui_core::Tree::from_view(description(&view), Default::default()).unwrap();
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(120.0, 60.0),
    )
    .unwrap();
    for scale in [1u32, 2] {
        let frame = Frame {
            extent: Extent::new(120 * scale, 60 * scale),
            scale: scale as f64,
            background: Color::BLACK,
        };
        let pixels = renderer.render(&tree.scene().unwrap(), frame).unwrap();
        let at = |x, y| {
            let offset = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
            pixels.pixels()[offset..offset + 4].to_vec()
        };
        assert_eq!(at(20, 29), [255, 0, 0, 255]);
        assert_eq!(at(100, 29), [255, 0, 0, 255]);
        assert_eq!(at(60, 29), [0, 0, 0, 255]);
        assert_eq!(at(5, 29), [0, 0, 0, 255]);
    }
    view.series = SparklineSeries::new(vec![Some(1.0), Some(1.0)]).unwrap();
    view.style.area = Some(Color::rgb(0, 255, 0));
    tree.reconcile(description(&view)).unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Paint);
    for scale in [1u32, 2] {
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(120 * scale, 60 * scale),
                    scale: scale as f64,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let offset = (30 * scale) as usize * pixels.stride() + (60 * scale * 4) as usize;
        assert_eq!(&pixels.pixels()[offset..offset + 4], &[0, 255, 0, 255]);
    }
    for samples in [vec![None, None], Vec::new()] {
        view.series = SparklineSeries::new(samples).unwrap();
        tree.reconcile(description(&view)).unwrap();
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(120, 60),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        assert!(
            pixels
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [0, 0, 0, 255])
        );
    }
    view.range = None;
    view.series = SparklineSeries::new(vec![Some(4.0)]).unwrap();
    tree.reconcile(description(&view)).unwrap();
    let pixels = renderer
        .render(
            &tree.scene().unwrap(),
            Frame {
                extent: Extent::new(120, 60),
                scale: 1.0,
                background: Color::BLACK,
            },
        )
        .unwrap();
    let offset = 30 * pixels.stride() + 60 * 4;
    assert_eq!(&pixels.pixels()[offset..offset + 4], &[255, 0, 0, 255]);
    view.style.line_width = f64::NAN;
    tree.reconcile(description(&view)).unwrap();
    assert!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(120.0, 60.0)
        )
        .is_err()
    );
    view.style.line_width = 2.0;
    tree.reconcile(description(&view)).unwrap();
    assert!(
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(120.0, 60.0)
        )
        .is_ok()
    );
}
#[cfg(feature = "cpu")]
#[test]
fn sparkline_gaps_area_empty_single_point_and_updates_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn sparkline_gaps_area_empty_single_point_and_updates_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

#[cfg(feature = "cpu")]
#[test]
fn bounded_history_snapshots_update_plot_without_changing_old_frames() {
    let mut history = zui_charts::SparklineHistory::new(2).unwrap();
    history.extend(&[Some(0.0), Some(0.0)]).unwrap();
    let old = history.snapshot();
    let mut view = SparklineView::themed(&Theme::dashboard(), old.clone());
    view.range = Some(ChartRange::new(0.0, 1.0).unwrap());
    view.style.line = Color::rgb(255, 0, 0);
    view.style.area = None;
    let mut tree = zui_core::Tree::from_view(description(&view), Default::default()).unwrap();
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(120.0, 60.0),
    )
    .unwrap();
    let mut renderer = zui_render_cpu::CpuRenderer::default();
    let frame = Frame {
        extent: Extent::new(120, 60),
        scale: 1.0,
        background: Color::BLACK,
    };
    let before = renderer.render(&tree.scene().unwrap(), frame).unwrap();
    history.extend(&[Some(1.0), Some(1.0)]).unwrap();
    view.series = history.snapshot();
    tree.reconcile(description(&view)).unwrap();
    assert_eq!(tree.invalidation(), Invalidation::Paint);
    let after = renderer.render(&tree.scene().unwrap(), frame).unwrap();
    let at = |pixels: &zui_paint::PixelBuffer, y| {
        let offset = y * pixels.stride() + 60 * 4;
        pixels.pixels()[offset..offset + 4].to_vec()
    };
    assert_eq!(at(&before, 48), [255, 0, 0, 255]);
    assert_eq!(at(&after, 48), [0, 0, 0, 255]);
    assert_eq!(at(&after, 10), [255, 0, 0, 255]);
    view.series = old;
    tree.reconcile(description(&view)).unwrap();
    let restored = renderer.render(&tree.scene().unwrap(), frame).unwrap();
    assert_eq!(before.pixels(), restored.pixels());
}
