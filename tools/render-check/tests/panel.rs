use zui_api::{
    layout::{Dimension, Edges},
    view::View,
    view::ViewSpec,
    widget::{Invalidation, PaintContext, Widget},
};
use zui_dashboard::{DashboardPanelView, PanelStyle};
use zui_paint::{Frame, OffscreenRenderer, Painter};
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Rect, Size};
#[derive(Clone, Copy)]
struct Content;
impl ViewSpec for Content {
    type Widget = Self;
    fn build(&self) -> Self {
        *self
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
impl Widget for Content {
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        painter.fill_rect(
            Rect::new(0.0, 0.0, context.size.width, context.size.height),
            Color::WHITE,
        );
    }
}
fn description(style: PanelStyle, size: Size, content: bool) -> View {
    let mut panel = DashboardPanelView::themed(&Theme::dark(), []);
    panel.style = style;
    panel.layout.width = Dimension::Length(size.width as f32);
    panel.layout.height = Dimension::Length(size.height as f32);
    panel.layout.padding = if content {
        Edges::all(20.0)
    } else {
        Edges::all(0.0)
    };
    if content {
        panel
            .children
            .push(View::new(Content).styled(zui_api::layout::LayoutStyle {
                width: Dimension::Length(20.0),
                height: Dimension::Length(10.0),
                ..Default::default()
            }));
    }
    panel.build()
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut style = PanelStyle::from_theme(&Theme::dark());
    style.background = Color::rgb(255, 0, 0);
    style.border = Color::rgb(0, 255, 0);
    style.accent = Color::rgb(0, 0, 255);
    style.border_width = 2.0;
    let size = Size::new(100.0, 60.0);
    let mut tree =
        zui_core::Tree::from_view(description(style, size, true), Default::default()).unwrap();
    tree.layout(&mut zui_layout::TaffyLayout::default(), size)
        .unwrap();
    for scale in [1u32, 2] {
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(100 * scale, 60 * scale),
                    scale: scale as f64,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let at = |x, y| {
            let offset = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
            pixels.pixels()[offset..offset + 4].to_vec()
        };
        assert_eq!(at(0, 0), [0, 0, 0, 255]);
        assert_eq!(at(50, 30), [255, 0, 0, 255]);
        assert_eq!(at(20, 0), [0, 0, 255, 255]);
        assert_eq!(at(60, 0), [0, 255, 0, 255]);
        assert_eq!(at(80, 59), [0, 0, 255, 255]);
        assert_eq!(at(25, 25), [255, 255, 255, 255]);
    }
    let target = tree
        .dispatch_input(zui_api::event::InputEvent::PointerMoved(Point::new(
            25.0, 25.0,
        )))
        .unwrap()
        .target;
    assert_eq!(target, Some(tree.children(tree.root()).unwrap()[0]));
    let target = tree
        .dispatch_input(zui_api::event::InputEvent::PointerMoved(Point::new(
            50.0, 30.0,
        )))
        .unwrap()
        .target;
    assert!(target.is_none());
    style.corner = f64::NAN;
    tree.reconcile(description(style, size, false)).unwrap();
    assert!(
        tree.layout(&mut zui_layout::TaffyLayout::default(), size)
            .is_err()
    );
    style.corner = 999.0;
    for size in [Size::new(2.0, 2.0), Size::ZERO] {
        tree.reconcile(description(style, size, false)).unwrap();
        tree.layout(
            &mut zui_layout::TaffyLayout::default(),
            Size::new(100.0, 60.0),
        )
        .unwrap();
        renderer
            .render(
                &tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(100, 60),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap();
    }
}
#[cfg(feature = "cpu")]
#[test]
fn panel_chamfers_accents_content_and_degenerate_sizes_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn panel_chamfers_accents_content_and_degenerate_sizes_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
