use std::sync::{Arc, OnceLock};
use zui_api::{
    layout::{Dimension, Edges, LayoutError, LayoutStyle},
    text::{TextAlign, TextError, TextLayout, TextRequest, TextService, TextStyle, TextWidth},
    view::View,
    widget::Invalidation,
};
use zui_core::{Tree, TreeError, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Frame, OffscreenRenderer, Scene};
use zui_text::ParleyText;
use zui_types::{Affine, Color, Extent, Size};
use zui_widgets::TextView;

struct Service {
    inner: ParleyText,
    calls: usize,
    epoch: u64,
    fail: bool,
    last_width: Option<TextWidth>,
}
impl TextService for Service {
    fn font_epoch(&self) -> u64 {
        self.epoch
    }
    fn layout(&mut self, request: TextRequest<'_>) -> Result<TextLayout, TextError> {
        self.calls += 1;
        self.last_width = Some(request.width);
        if self.fail {
            return Err(TextError::InvalidFont);
        }
        self.inner.layout(request)
    }
}
fn setup() -> (Service, TextView) {
    static FONT: OnceLock<Arc<[u8]>> = OnceLock::new();
    let mut inner = ParleyText::default();
    let families = inner
        .register_font(
            FONT.get_or_init(|| {
                Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice())
            })
            .clone(),
        )
        .unwrap();
    let view = TextView::new(
        "中文布局 Rust wraps into multiple lines，宽度改变时重新测量。",
        TextStyle {
            families,
            font_size: 20.0,
            line_height: 1.4,
            align: TextAlign::End,
        },
        Color::WHITE,
    );
    (
        Service {
            inner,
            calls: 0,
            epoch: 1,
            fail: false,
            last_width: None,
        },
        view,
    )
}
fn description(text: &TextView, fixed_height: bool) -> View {
    View::new(text.clone()).keyed("text").styled(LayoutStyle {
        width: Dimension::Percent(1.0),
        height: if fixed_height {
            Dimension::Length(220.0)
        } else {
            Dimension::Auto
        },
        padding: Edges::all(10.0),
        ..Default::default()
    })
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let (mut service, view) = setup();
    let mut tree = Tree::from_view(description(&view, false), TreeLimits::default()).unwrap();
    let mut engine = TaffyLayout::default();
    let mut previous_height = 0.0;
    for width in [300.0, 160.0] {
        tree.layout_with_text(&mut engine, &mut service, Size::new(width, 300.0))
            .unwrap();
        let bounds = tree.layout_box(tree.root()).unwrap().bounds;
        assert_eq!(bounds.size.width, width);
        assert!(bounds.size.height > previous_height);
        previous_height = bounds.size.height;
        assert_eq!(
            service.last_width,
            Some(TextWidth::AtMost(width as f32 - 20.0))
        );
        let shaped = service
            .inner
            .layout(TextRequest {
                text: &view.text,
                style: &view.style,
                width: TextWidth::AtMost(width as f32 - 20.0),
            })
            .unwrap();
        assert!((bounds.size.height - shaped.size.height - 20.0).abs() < 0.01);
        let mut expected = Scene::new();
        expected
            .painter()
            .with_transform(Affine::translate(10.0, 10.0), |p| {
                shaped.paint(p, view.color)
            });
        let actual = tree.scene().unwrap();
        for scale in [1.0, 2.0] {
            let frame = Frame {
                extent: Extent::new((width * scale) as u32, (300.0 * scale) as u32),
                scale,
                background: Color::BLACK,
            };
            let a = renderer.render(&actual, frame).unwrap();
            let b = renderer.render(&expected, frame).unwrap();
            assert_eq!(
                a.pixels(),
                b.pixels(),
                "prepared text differs from final-width layout"
            );
            assert!(
                a.pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| *p != [0, 0, 0, 255])
            );
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn text_widget_reflows_and_paints_final_content_box_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn text_widget_reflows_and_paints_final_content_box_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
#[test]
fn fixed_size_prepares_text_and_updates_invalidate_only_what_changed() {
    let (mut service, mut view) = setup();
    let mut tree = Tree::from_view(description(&view, true), TreeLimits::default()).unwrap();
    let mut engine = TaffyLayout::default();
    let viewport = Size::new(200.0, 300.0);
    tree.layout_with_text(&mut engine, &mut service, viewport)
        .unwrap();
    assert!(
        service.calls > 0,
        "definite dimensions must still prepare glyphs"
    );
    assert_eq!(service.last_width, Some(TextWidth::AtMost(180.0)));
    let first = tree.scene().unwrap();
    assert!(!first.commands().is_empty());
    let calls = service.calls;
    view.color = Color::rgb(10, 150, 200);
    let report = tree.reconcile(description(&view, true)).unwrap();
    assert_eq!(report.invalidation, Invalidation::Paint);
    assert!(
        !tree
            .layout_with_text(&mut engine, &mut service, viewport)
            .unwrap()
    );
    assert_eq!(service.calls, calls);
    let changed = tree.scene().unwrap();
    assert_ne!(first.commands(), changed.commands());
    service.epoch += 1;
    assert!(
        tree.layout_with_text(&mut engine, &mut service, viewport)
            .unwrap()
    );
    assert!(service.calls > calls);
    view.text.push_str(" 新内容");
    assert_eq!(
        tree.reconcile(description(&view, true))
            .unwrap()
            .invalidation,
        Invalidation::Layout
    );
    assert!(matches!(tree.scene(), Err(TreeError::LayoutRequired)));
    tree.layout_with_text(&mut engine, &mut service, viewport)
        .unwrap();
    assert!(tree.scene().is_ok());
}
#[test]
fn unavailable_or_failed_preparation_blocks_publication_and_can_recover() {
    let (mut service, view) = setup();
    let mut tree = Tree::from_view(description(&view, true), TreeLimits::default()).unwrap();
    let mut engine = TaffyLayout::default();
    let viewport = Size::new(200.0, 300.0);
    assert!(matches!(
        tree.layout(&mut engine, viewport),
        Err(TreeError::Layout(LayoutError::Text(
            _,
            TextError::ServiceUnavailable
        )))
    ));
    tree.layout_with_text(&mut engine, &mut service, viewport)
        .unwrap();
    tree.scene().unwrap();
    service.fail = true;
    tree.invalidate(Invalidation::Layout);
    assert!(matches!(
        tree.layout_with_text(&mut engine, &mut service, viewport),
        Err(TreeError::Layout(LayoutError::Text(
            _,
            TextError::InvalidFont
        )))
    ));
    assert!(matches!(tree.scene(), Err(TreeError::LayoutRequired)));
    assert!(matches!(
        tree.layout_box(tree.root()),
        Err(TreeError::LayoutRequired)
    ));
    service.fail = false;
    tree.layout_with_text(&mut engine, &mut service, viewport)
        .unwrap();
    assert!(tree.scene().is_ok());
}
