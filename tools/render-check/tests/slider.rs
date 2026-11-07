use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_core::{Tree, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{NumericRange, SliderChanged, SliderView};
struct Fixture {
    view: SliderView,
    tree: Tree,
    engine: TaffyLayout,
    width: f32,
}
impl Fixture {
    fn new() -> Self {
        let view = SliderView::themed(
            &Theme::dashboard(),
            NumericRange::new(0.0, 100.0, 5.0).unwrap(),
            50.0,
            ActionId(4),
        );
        let tree = Tree::from_view(description(view, 236.0), TreeLimits::default()).unwrap();
        let mut f = Self {
            view,
            tree,
            engine: Default::default(),
            width: 236.0,
        };
        f.prepare();
        f
    }
    fn prepare(&mut self) -> bool {
        self.tree
            .layout(&mut self.engine, Size::new(300.0, 100.0))
            .unwrap()
    }
    fn update(&mut self) -> bool {
        self.tree
            .reconcile(description(self.view, self.width))
            .unwrap();
        self.prepare()
    }
    fn input(&mut self, input: InputEvent) -> zui_core::DispatchResult {
        self.prepare();
        self.tree.dispatch_input(input).unwrap()
    }
}
fn description(view: SliderView, width: f32) -> View {
    View::new(view).styled(LayoutStyle {
        width: Dimension::Length(width),
        height: Dimension::Length(40.0),
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
fn moved(x: f64) -> InputEvent {
    InputEvent::PointerMoved(Point::new(x, 20.0))
}
fn key(key: Key) -> InputEvent {
    InputEvent::Key(KeyInput {
        key,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    })
}
fn requested(result: zui_core::DispatchResult) -> f64 {
    assert_eq!(result.actions.len(), 1);
    assert_eq!(result.actions[0].action.id, ActionId(4));
    result.actions[0]
        .action
        .payload::<SliderChanged>()
        .unwrap()
        .value
}
#[test]
fn dragging_keeps_capture_across_model_acknowledgements_and_clamps_outside() {
    let mut f = Fixture::new();
    assert!(f.input(mouse(true, 118.0)).actions.is_empty());
    let id = f.tree.root();
    assert_eq!(f.tree.pointer_capture(), Some(id));
    assert_eq!(requested(f.input(moved(158.0))), 70.0);
    assert_eq!(f.view.value, 50.0);
    f.view.value = 70.0;
    assert!(!f.update());
    assert_eq!(f.tree.pointer_capture(), Some(id));
    assert!(f.input(moved(158.0)).actions.is_empty());
    assert_eq!(requested(f.input(moved(198.0))), 90.0);
    f.view.value = 90.0;
    f.update();
    assert_eq!(requested(f.input(moved(400.0))), 100.0);
    assert_eq!(requested(f.input(mouse(false, -100.0))), 0.0);
    assert_eq!(f.tree.pointer_capture(), None);
    assert!(f.input(moved(100.0)).actions.is_empty());
}
#[test]
fn thumb_offset_padding_external_overrides_and_escape_have_explicit_behavior() {
    let mut f = Fixture::new();
    f.view.value = 53.0;
    f.update();
    assert!(f.input(mouse(true, 124.0)).actions.is_empty());
    assert!(f.input(mouse(false, 124.0)).actions.is_empty());
    f.view.value = 50.0;
    f.update();
    assert!(f.input(mouse(true, 2.0)).actions.is_empty());
    assert_eq!(f.tree.pointer_capture(), None);
    assert!(f.input(mouse(true, 123.0)).actions.is_empty()); // 5 units right of thumb center
    assert_eq!(requested(f.input(moved(163.0))), 70.0); // no initial jump
    f.view.value = 20.0;
    f.update(); // external override cancels drag
    assert!(f.input(moved(200.0)).actions.is_empty());
    f.input(mouse(false, 200.0));
    assert_eq!(requested(f.input(mouse(true, 218.0))), 100.0); // track click
    assert!(f.input(key(Key::Escape)).handled);
    assert_eq!(f.tree.pointer_capture(), None);
    assert!(f.input(mouse(false, 18.0)).actions.is_empty());
    f.input(mouse(true, 58.0));
    f.tree.cancel_input();
    assert!(f.input(mouse(false, 218.0)).actions.is_empty());
}
#[test]
fn keyboard_steps_handle_partial_endpoints_decimal_values_and_modifiers() {
    let mut f = Fixture::new();
    f.input(key(Key::Tab));
    assert_eq!(requested(f.input(key(Key::ArrowRight))), 55.0);
    assert_eq!(requested(f.input(key(Key::PageDown))), 0.0);
    assert_eq!(requested(f.input(key(Key::End))), 100.0);
    f.view.range = NumericRange::new(0.0, 10.0, 3.0).unwrap();
    f.view.value = 10.0;
    f.update();
    assert_eq!(requested(f.input(key(Key::ArrowLeft))), 9.0);
    f.view.value = 9.0;
    f.update();
    assert_eq!(requested(f.input(key(Key::ArrowRight))), 10.0);
    f.view.range = NumericRange::new(-1.0, 1.0, 0.1).unwrap();
    f.view.value = 0.3;
    f.update();
    assert!((requested(f.input(key(Key::ArrowRight))) - 0.4).abs() < 1e-12);
    let mut modified = key(Key::ArrowLeft);
    if let InputEvent::Key(k) = &mut modified {
        k.modifiers.control = true;
    }
    assert!(!f.input(modified).handled);
    assert!(!f.input(key(Key::Space)).handled);
    f.view.disabled = true;
    f.update();
    assert!(f.tree.focused().is_none());
    assert!(f.input(mouse(true, 100.0)).actions.is_empty());
}
#[test]
fn invalid_ranges_values_and_styles_do_not_publish_invalid_scenes() {
    for (min, max, step) in [
        (0.0, 0.0, 1.0),
        (1.0, 0.0, 1.0),
        (0.0, 1.0, 0.0),
        (0.0, 1.0, 2.0),
        (0.0, f64::INFINITY, 1.0),
        (-f64::MAX, f64::MAX, 1.0),
        (0.0, 1.0, f64::MIN_POSITIVE),
    ] {
        assert!(NumericRange::new(min, max, step).is_err());
    }
    let mut f = Fixture::new();
    f.view.value = f64::NAN;
    f.tree.reconcile(description(f.view, f.width)).unwrap();
    assert!(
        f.tree
            .layout(&mut f.engine, Size::new(300.0, 100.0))
            .is_err()
    );
    assert!(f.tree.scene().is_err());
    f.view.value = 50.0;
    f.update();
    f.view.style.track_thickness = 30.0;
    f.tree.reconcile(description(f.view, f.width)).unwrap();
    assert!(
        f.tree
            .layout(&mut f.engine, Size::new(300.0, 100.0))
            .is_err()
    );
    f.view.style.track_thickness = 4.0;
    f.width = 30.0;
    f.update();
    assert!(f.input(mouse(true, 15.0)).actions.is_empty());
    assert_eq!(f.tree.pointer_capture(), None);
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    for disabled in [false, true] {
        f.view.disabled = disabled;
        f.update();
        let colors = if disabled {
            f.view.style.disabled
        } else {
            f.view.style.normal
        };
        let scene = f.tree.scene().unwrap();
        for scale in [1.0, 2.0] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new((260.0 * scale) as u32, (60.0 * scale) as u32),
                        scale,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for (x, color) in [
                (50.0, colors.fill),
                (118.0, colors.thumb),
                (190.0, colors.track),
                (2.0, Color::BLACK),
            ] {
                let offset = (20.0 * scale) as usize * pixels.stride() + (x * scale) as usize * 4;
                assert_eq!(
                    &pixels.pixels()[offset..offset + 4],
                    &[color.r, color.g, color.b, color.a]
                );
            }
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn slider_track_thumb_and_disabled_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn slider_track_thumb_and_disabled_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
