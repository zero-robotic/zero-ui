use std::sync::{Arc, OnceLock};
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    text::{TextRequest, TextService, TextWidth},
    view::View,
};
use zui_core::{Tree, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Frame, OffscreenRenderer};
use zui_text::ParleyText;
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{CheckBoxView, CheckChanged, CheckState};
struct Fixture {
    tree: Tree,
    engine: TaffyLayout,
    text: ParleyText,
    view: CheckBoxView,
    width: f32,
    height: Dimension,
}
impl Fixture {
    fn new() -> Self {
        static FONT: OnceLock<Arc<[u8]>> = OnceLock::new();
        let mut text = ParleyText::default();
        let families = text
            .register_font(
                FONT.get_or_init(|| {
                    Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice())
                })
                .clone(),
            )
            .unwrap();
        let view = CheckBoxView::themed(
            &Theme::dashboard(),
            "选择 Checkbox",
            families,
            CheckState::Unchecked,
            ActionId(2),
        );
        let tree = Tree::from_view(
            description(&view, 220.0, Dimension::Length(48.0)),
            TreeLimits::default(),
        )
        .unwrap();
        let mut f = Self {
            tree,
            engine: Default::default(),
            text,
            view,
            width: 220.0,
            height: Dimension::Length(48.0),
        };
        f.prepare();
        f
    }
    fn prepare(&mut self) -> bool {
        self.tree
            .layout_with_text(&mut self.engine, &mut self.text, Size::new(240.0, 360.0))
            .unwrap()
    }
    fn update(&mut self) -> bool {
        self.tree
            .reconcile(description(&self.view, self.width, self.height))
            .unwrap();
        self.prepare()
    }
    fn input(&mut self, input: InputEvent) -> zui_core::DispatchResult {
        self.prepare();
        self.tree.dispatch_input(input).unwrap()
    }
}
fn description(view: &CheckBoxView, width: f32, height: Dimension) -> View {
    View::new(view.clone()).keyed("check").styled(LayoutStyle {
        width: Dimension::Length(width),
        height,
        padding: Edges::all(8.0),
        ..Default::default()
    })
}
fn mouse(pressed: bool, x: f64) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(x, 24.0)),
    }
}
fn key(key: Key, pressed: bool) -> InputEvent {
    InputEvent::Key(KeyInput {
        key,
        pressed,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    })
}
fn request(result: zui_core::DispatchResult) -> CheckState {
    assert_eq!(result.actions.len(), 1);
    let event = &result.actions[0];
    assert_eq!(event.action.id, ActionId(2));
    event.action.payload::<CheckChanged>().unwrap().state
}
#[test]
fn label_click_requests_value_and_only_application_updates_the_selection() {
    let mut f = Fixture::new();
    let id = f.tree.root();
    for _ in 0..2 {
        f.input(mouse(true, 160.0));
        assert_eq!(request(f.input(mouse(false, 160.0))), CheckState::Checked);
        assert_eq!(f.view.state, CheckState::Unchecked);
    }
    f.tree.scene().unwrap();
    f.view.state = CheckState::Checked;
    assert!(!f.update(), "value update should not remeasure text");
    assert_eq!(f.tree.root(), id);
    f.input(mouse(true, 12.0));
    assert_eq!(request(f.input(mouse(false, 12.0))), CheckState::Unchecked);
    f.view.state = CheckState::Mixed;
    f.update();
    f.input(mouse(true, 160.0));
    assert_eq!(request(f.input(mouse(false, 160.0))), CheckState::Checked);
}
#[test]
fn keyboard_external_value_change_and_disabling_cancel_pending_gestures() {
    let mut f = Fixture::new();
    f.input(key(Key::Tab, true));
    let enter = f.input(key(Key::Enter, true));
    assert!(!enter.handled);
    assert!(enter.actions.is_empty());
    f.input(key(Key::Space, true));
    assert_eq!(
        request(f.input(key(Key::Space, false))),
        CheckState::Checked
    );
    f.input(key(Key::Space, true));
    f.view.state = CheckState::Checked;
    f.update();
    assert!(f.input(key(Key::Space, false)).actions.is_empty());
    f.input(mouse(true, 20.0));
    assert!(f.input(mouse(false, 235.0)).actions.is_empty());
    f.input(mouse(true, 20.0));
    f.view.disabled = true;
    f.update();
    assert_eq!(f.tree.focused(), None);
    assert_eq!(f.tree.pointer_capture(), None);
    assert!(f.input(mouse(false, 20.0)).actions.is_empty());
    assert!(f.input(mouse(true, 20.0)).handled);
    assert!(f.input(mouse(false, 20.0)).actions.is_empty());
}
#[test]
fn label_measurement_reserves_indicator_space_and_rejects_invalid_metrics() {
    let mut f = Fixture::new();
    f.view.label = "选择需要换行的中文标签，Rust checkbox uses remaining content width".into();
    f.height = Dimension::Auto;
    let mut previous = 0.0;
    for width in [220.0, 140.0] {
        f.width = width;
        f.update();
        let expected = f
            .text
            .layout(TextRequest {
                text: &f.view.label,
                style: &f.view.text_style,
                width: TextWidth::AtMost(
                    width - 16.0 - f.view.style.indicator_size - f.view.style.gap,
                ),
            })
            .unwrap();
        let height = f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height;
        assert!(
            (height - 16.0 - expected.size.height.max(f.view.style.indicator_size as f64)).abs()
                < 0.01
        );
        assert!(height > previous);
        previous = height;
    }
    f.view.style.indicator_size = f32::NAN;
    f.tree
        .reconcile(description(&f.view, f.width, f.height))
        .unwrap();
    assert!(
        f.tree
            .layout_with_text(&mut f.engine, &mut f.text, Size::new(240.0, 360.0))
            .is_err()
    );
    assert!(f.tree.scene().is_err());
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    for (state, disabled) in [
        (CheckState::Unchecked, false),
        (CheckState::Checked, false),
        (CheckState::Mixed, false),
        (CheckState::Mixed, true),
    ] {
        f.view.state = state;
        f.view.disabled = disabled;
        f.update();
        let scene = f.tree.scene().unwrap();
        let colors = if disabled {
            f.view.style.disabled
        } else {
            f.view.style.normal
        };
        for scale in [1.0, 2.0] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new((240.0 * scale) as u32, (80.0 * scale) as u32),
                        scale,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let sample = |x: usize, y: usize| {
                let offset = y * scale as usize * pixels.stride() + x * scale as usize * 4;
                &pixels.pixels()[offset..offset + 4]
            };
            let background = if state == CheckState::Unchecked {
                colors.background
            } else {
                colors.selected_background
            };
            assert_eq!(
                sample(10, 16),
                &[background.r, background.g, background.b, background.a]
            );
            let mut mark_pixels = 0;
            for y in 16 * scale as usize..32 * scale as usize {
                for x in 10 * scale as usize..26 * scale as usize {
                    let offset = y * pixels.stride() + x * 4;
                    if pixels.pixels()[offset..offset + 3]
                        == [colors.mark.r, colors.mark.g, colors.mark.b]
                    {
                        mark_pixels += 1;
                    }
                }
            }
            assert_eq!(mark_pixels > 0, state != CheckState::Unchecked);
            assert_eq!(sample(230, 24), &[0, 0, 0, 255]);
            assert!(
                pixels
                    .pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[..3] == [colors.text.r, colors.text.g, colors.text.b])
            );
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn checkbox_tristate_and_disabled_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn checkbox_tristate_and_disabled_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
