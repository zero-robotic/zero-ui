use zui_api::{
    layout::{Dimension, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::{ChildViewport, Invalidation, PaintContext, Widget},
};
use zui_core::Tree;
use zui_custom_widget_example::MeterView;
use zui_paint::{Frame, OffscreenRenderer, Painter};
use zui_types::{Color, Extent, Point, Rect, Size};

#[derive(Clone, Copy)]
struct Viewport(f64);
impl Widget for Viewport {
    fn child_viewport(&self, _: PaintContext) -> ChildViewport {
        ChildViewport {
            translation: Point::new(0.0, -self.0),
            clip: Some(Rect::new(10.0, 10.0, 40.0, 30.0)),
        }
    }
    fn paint(&self, _: PaintContext, painter: &mut Painter<'_>) {
        painter.fill_rect(Rect::new(0.0, 0.0, 60.0, 50.0), Color::WHITE);
    }
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
}
impl ViewSpec for Viewport {
    type Widget = Self;
    fn build(&self) -> Self {
        *self
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        *widget = *self;
        Invalidation::Paint
    }
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let red = Color::rgb(255, 0, 0);
    let child = View::new(MeterView::new(1.0, red, red).unwrap()).styled(LayoutStyle {
        width: Dimension::Length(40.0),
        height: Dimension::Length(40.0),
        shrink: 0.0,
        margin: zui_api::layout::Edges {
            left: 10.0,
            top: 20.0,
            ..Default::default()
        },
        ..Default::default()
    });
    let view = View::new(Viewport(0.0))
        .styled(LayoutStyle {
            kind: LayoutKind::Column,
            width: Dimension::Length(60.0),
            height: Dimension::Length(50.0),
            ..Default::default()
        })
        .with_children([child]);
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(80.0, 60.0),
    )
    .unwrap();
    let child = tree.children(tree.root()).unwrap()[0];
    assert_eq!(
        tree.layout_box(child).unwrap().bounds,
        Rect::new(10.0, 20.0, 40.0, 40.0)
    );
    for offset in [0.0, 25.0, 60.0] {
        tree.update_widget::<Viewport>(tree.root(), Invalidation::Paint, |v| v.0 = offset)
            .unwrap();
        for scale in [1, 2] {
            let pixels = renderer
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(80 * scale, 60 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for y in 0..60 * scale as usize {
                for x in 0..80 * scale as usize {
                    let logical = Point::new(x as f64 / scale as f64, y as f64 / scale as f64);
                    let visible = Rect::new(10.0, 10.0, 40.0, 30.0).contains(logical)
                        && Rect::new(10.0, 20.0 - offset, 40.0, 40.0).contains(logical);
                    let expected = if visible {
                        red
                    } else if Rect::new(0.0, 0.0, 60.0, 50.0).contains(logical) {
                        Color::WHITE
                    } else {
                        Color::BLACK
                    };
                    let i = y * pixels.stride() + x * 4;
                    assert_eq!(
                        &pixels.pixels()[i..i + 4],
                        &[expected.r, expected.g, expected.b, 255]
                    );
                    assert_eq!(tree.hit_test(logical).unwrap().is_some(), visible);
                }
            }
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn child_translation_and_stationary_clip_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn child_translation_and_stationary_clip_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
