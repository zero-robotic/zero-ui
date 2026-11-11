use std::sync::{Arc, OnceLock};
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_core::{Tree, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Frame, OffscreenRenderer};
use zui_text::ParleyText;
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{ButtonActivated, ButtonView};

struct Fixture {
    tree: Tree,
    layout: TaffyLayout,
    text: ParleyText,
    view: ButtonView,
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
        let view = ButtonView::themed(&Theme::dashboard(), "按钮 Activate", families, ActionId(7));
        let tree = Tree::from_view(description(&view), TreeLimits::default()).unwrap();
        let mut fixture = Self {
            tree,
            layout: TaffyLayout::default(),
            text,
            view,
        };
        fixture.prepare();
        fixture
    }
    fn prepare(&mut self) {
        self.tree
            .layout_with_text(&mut self.layout, &mut self.text, Size::new(240.0, 80.0))
            .unwrap();
    }
    fn update(&mut self) {
        self.tree.reconcile(description(&self.view)).unwrap();
        self.prepare();
    }
    fn input(&mut self, input: InputEvent) -> zui_core::DispatchResult {
        self.prepare();
        self.tree.dispatch_input(input).unwrap()
    }
}
fn description(view: &ButtonView) -> View {
    View::new(view.clone()).keyed("button").styled(LayoutStyle {
        width: Dimension::Length(200.0),
        height: Dimension::Length(48.0),
        padding: Edges::all(8.0),
        ..Default::default()
    })
}
fn mouse(pressed: bool, x: f64) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(x, 20.0)),
    }
}
fn key(key: Key, pressed: bool, repeat: bool) -> InputEvent {
    InputEvent::Key(KeyInput {
        key,
        pressed,
        repeat,
        text: None,
        modifiers: Default::default(),
    })
}
fn activated(result: zui_core::DispatchResult) -> usize {
    for event in &result.actions {
        assert_eq!(event.action.id, ActionId(7));
        assert!(event.action.payload::<ButtonActivated>().is_some());
    }
    result.actions.len()
}
#[test]
fn mouse_click_drag_out_and_back_and_disabled_press_are_safe() {
    let mut f = Fixture::new();
    assert_eq!(activated(f.input(mouse(true, 20.0))), 0);
    assert_eq!(f.tree.pointer_capture(), Some(f.tree.root()));
    assert_eq!(activated(f.input(mouse(false, 20.0))), 1);
    assert_eq!(f.tree.pointer_capture(), None);
    f.input(mouse(true, 20.0));
    assert_eq!(activated(f.input(mouse(false, 220.0))), 0);
    f.input(mouse(true, 20.0));
    f.input(InputEvent::PointerMoved(Point::new(220.0, 20.0)));
    f.input(InputEvent::PointerMoved(Point::new(20.0, 20.0)));
    assert_eq!(activated(f.input(mouse(false, 20.0))), 1);
    f.input(mouse(true, 20.0));
    f.view.disabled = true;
    f.update();
    assert_eq!(f.tree.pointer_capture(), None);
    assert_eq!(f.tree.focused(), None);
    assert_eq!(activated(f.input(mouse(false, 20.0))), 0);
    assert!(f.input(mouse(true, 20.0)).handled);
    assert_eq!(activated(f.input(mouse(false, 20.0))), 0);
    f.input(key(Key::Tab, true, false));
    assert_eq!(f.tree.focused(), None);
}
#[test]
fn keyboard_activation_ignores_repeat_and_cancellation_prevents_late_activation() {
    let mut f = Fixture::new();
    f.input(key(Key::Tab, true, false));
    assert_eq!(f.tree.focused(), Some(f.tree.root()));
    assert_eq!(activated(f.input(key(Key::Space, true, false))), 0);
    assert_eq!(activated(f.input(key(Key::Space, true, true))), 0);
    assert_eq!(activated(f.input(key(Key::Space, false, false))), 1);
    assert_eq!(activated(f.input(key(Key::Space, false, false))), 0);
    assert_eq!(activated(f.input(key(Key::Enter, true, false))), 1);
    assert_eq!(activated(f.input(key(Key::Enter, true, true))), 0);
    assert_eq!(activated(f.input(key(Key::Enter, false, false))), 0);
    f.input(key(Key::Space, true, false));
    f.input(key(Key::Escape, true, false));
    assert_eq!(activated(f.input(key(Key::Space, false, false))), 0);
    f.input(mouse(true, 20.0));
    f.input(key(Key::Escape, true, false));
    assert_eq!(f.tree.pointer_capture(), None);
    assert_eq!(activated(f.input(mouse(false, 20.0))), 0);
    f.input(key(Key::Space, true, false));
    f.tree.cancel_input();
    assert_eq!(activated(f.input(key(Key::Space, false, false))), 0);
}
#[test]
fn changing_binding_during_press_does_not_activate_new_action() {
    let mut f = Fixture::new();
    f.input(mouse(true, 20.0));
    f.view.action = ActionId(8);
    f.update();
    assert!(f.input(mouse(false, 20.0)).actions.is_empty());
    f.input(key(Key::Space, true, false));
    f.view.action = ActionId(9);
    f.update();
    assert!(f.input(key(Key::Space, false, false)).actions.is_empty());
}

#[test]
fn reserved_icon_slot_keeps_text_and_layout_stable_when_icon_changes() {
    use zui_api::{view::ViewSpec, widget::Invalidation};
    use zui_widgets::{Icon, IconKind};
    let mut fixture = Fixture::new();
    fixture.view.reserve_trailing_icon = true;
    fixture.update();
    let bounds = fixture.tree.layout_box(fixture.tree.root()).unwrap();
    let glyphs = |scene: zui_paint::Scene| {
        scene
            .commands()
            .iter()
            .filter(|command| matches!(command, zui_paint::Command::Glyphs { .. }))
            .cloned()
            .collect::<Vec<_>>()
    };
    let expected = glyphs(fixture.tree.scene().unwrap());
    let mut widget = fixture.view.build();
    for icon in [
        Some(Icon::builtin(IconKind::ChevronUp)),
        Some(Icon::builtin(IconKind::ChevronDown)),
        None,
    ] {
        fixture.view.trailing_icon = icon;
        assert_eq!(fixture.view.update(&mut widget), Invalidation::Paint);
        fixture.update();
        assert_eq!(
            fixture.tree.layout_box(fixture.tree.root()).unwrap(),
            bounds
        );
        assert_eq!(glyphs(fixture.tree.scene().unwrap()), expected);
    }
    fixture.view.reserve_trailing_icon = false;
    assert_eq!(fixture.view.update(&mut widget), Invalidation::Layout);
    fixture.view.trailing_icon = Some(Icon::builtin(IconKind::ChevronUp));
    assert_eq!(fixture.view.update(&mut widget), Invalidation::Layout);
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    for state in 0..5 {
        match state {
            1 => {
                f.input(InputEvent::PointerMoved(Point::new(20.0, 20.0)));
            }
            2 => {
                f.input(mouse(true, 20.0));
            }
            3 => {
                f.input(mouse(false, 20.0));
                f.input(InputEvent::PointerLeft);
            }
            4 => {
                f.view.disabled = true;
                f.update();
            }
            _ => {}
        }
        let expected = match state {
            1 => f.view.style.hovered,
            2 => f.view.style.pressed,
            4 => f.view.style.disabled,
            _ => f.view.style.normal,
        };
        f.prepare();
        let scene = f.tree.scene().unwrap();
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
            let c = expected.background;
            assert_eq!(sample(4, 4), &[c.r, c.g, c.b, c.a]);
            assert_eq!(sample(210, 20), &[0, 0, 0, 255]);
            if state == 2 || state == 3 {
                assert_eq!(sample(1, 20), &[c.r, c.g, c.b, c.a]);
            }
            assert!(
                pixels.pixels().as_chunks::<4>().0.iter().any(|p| p[..3]
                    == [
                        expected.foreground.r,
                        expected.foreground.g,
                        expected.foreground.b
                    ]),
                "label not rendered"
            );
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn button_states_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn button_states_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
