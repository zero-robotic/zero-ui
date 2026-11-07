use std::sync::Arc;
use zui_api::{
    text::{TextError, TextLayout, TextRequest, TextService},
    widget::Invalidation,
};
use zui_core::{Tree, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Command, Frame, OffscreenRenderer};
use zui_text::ParleyText;
use zui_theme::{ColorRole, Space, Theme, ThemeError, TypeRole, Typography};
use zui_types::{Color, Extent, Size};

struct CountingText {
    inner: ParleyText,
    calls: usize,
}
impl TextService for CountingText {
    fn font_epoch(&self) -> u64 {
        self.inner.font_epoch()
    }
    fn layout(&mut self, request: TextRequest<'_>) -> Result<TextLayout, TextError> {
        self.calls += 1;
        self.inner.layout(request)
    }
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut service = CountingText {
        inner: ParleyText::default(),
        calls: 0,
    };
    let families = service
        .inner
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut theme = Theme::dark();
    let mut tree = Tree::from_view(
        zui_render_check::theme_view(&theme, families.clone()),
        TreeLimits::default(),
    )
    .unwrap();
    let mut engine = TaffyLayout::default();
    let viewport = Size::new(640.0, 360.0);
    tree.layout_with_text(&mut engine, &mut service, viewport)
        .unwrap();
    let ids = tree.children(tree.root()).unwrap().to_vec();
    let original = tree.scene().unwrap();
    let mut previous = renderer
        .render(
            &original,
            Frame {
                extent: Extent::new(640, 360),
                scale: 1.0,
                background: Color::BLACK,
            },
        )
        .unwrap();
    let glyphs = |scene: &zui_paint::Scene| {
        scene
            .commands()
            .iter()
            .filter_map(|c| match c {
                Command::Glyphs { run, transform, .. } => Some((run.clone(), *transform)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for next in [Theme::light(), Theme::dashboard()] {
        theme = next;
        let calls = service.calls;
        let report = tree
            .reconcile(zui_render_check::theme_view(&theme, families.clone()))
            .unwrap();
        assert_eq!(report.invalidation, Invalidation::Paint);
        assert_eq!((report.created, report.removed), (0, 0));
        assert_eq!(tree.children(tree.root()).unwrap(), ids);
        assert!(
            !tree
                .layout_with_text(&mut engine, &mut service, viewport)
                .unwrap()
        );
        assert_eq!(service.calls, calls, "palette switch reshaped text");
        let scene = tree.scene().unwrap();
        assert_eq!(glyphs(&scene), glyphs(&original));
        let pixels = renderer
            .render(
                &scene,
                Frame {
                    extent: Extent::new(640, 360),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let color = theme.color(ColorRole::Background);
        assert_eq!(&pixels.pixels()[..4], &[color.r, color.g, color.b, 255]);
        assert_ne!(pixels.pixels(), previous.pixels());
        previous = pixels;
    }
    let calls = service.calls;
    theme.set_typography(TypeRole::Body, Typography::new(30.0, 1.5).unwrap());
    assert_eq!(
        tree.reconcile(zui_render_check::theme_view(&theme, families.clone()))
            .unwrap()
            .invalidation,
        Invalidation::Layout
    );
    tree.layout_with_text(&mut engine, &mut service, viewport)
        .unwrap();
    assert!(service.calls > calls);
    assert!(
        tree.scene()
            .unwrap()
            .commands()
            .iter()
            .any(|c| matches!(c, Command::Glyphs {run,..} if run.font_size == 30.0))
    );
    let before = tree.layout_box(ids[0]).unwrap().bounds;
    theme.set_space(Space::Lg, 36.0).unwrap();
    assert_eq!(
        tree.reconcile(zui_render_check::theme_view(&theme, families))
            .unwrap()
            .invalidation,
        Invalidation::Layout
    );
    tree.layout_with_text(&mut engine, &mut service, viewport)
        .unwrap();
    assert_ne!(
        tree.layout_box(ids[0]).unwrap().bounds.origin,
        before.origin
    );
}
#[cfg(feature = "cpu")]
#[test]
fn theme_switch_preserves_identity_and_selects_invalidation_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn theme_switch_preserves_identity_and_selects_invalidation_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
#[test]
fn invalid_tokens_do_not_mutate_theme_and_overrides_are_local() {
    let original = Theme::dark();
    let mut local = original.clone();
    for invalid in [f32::NAN, f32::INFINITY, -1.0] {
        assert_eq!(
            local.set_space(Space::Md, invalid),
            Err(ThemeError::InvalidSpacing)
        );
        assert_eq!(local, original);
        assert_eq!(
            Typography::new(invalid, 1.4),
            Err(ThemeError::InvalidTypography)
        );
        assert_eq!(
            Typography::new(18.0, invalid),
            Err(ThemeError::InvalidTypography)
        );
    }
    assert!(Typography::new(0.0, 1.4).is_err());
    assert!(Typography::new(18.0, 0.0).is_err());
    local.set_color(ColorRole::Accent, Color::WHITE);
    assert_ne!(
        local.color(ColorRole::Accent),
        original.color(ColorRole::Accent)
    );
    assert_eq!(
        local.typography(TypeRole::Body),
        original.typography(TypeRole::Body)
    );
}
