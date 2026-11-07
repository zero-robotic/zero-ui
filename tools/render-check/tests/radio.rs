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
use zui_widgets::{RadioChanged, RadioGroupView, RadioOption, RadioOptionId};
struct Fixture {
    tree: Tree,
    engine: TaffyLayout,
    text: ParleyText,
    view: RadioGroupView,
    width: f32,
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
        let mut view = RadioGroupView::themed(
            &Theme::dashboard(),
            vec![
                RadioOption::new(RadioOptionId(10), "自动 Auto"),
                RadioOption::new(RadioOptionId(20), "停用 Disabled"),
                RadioOption::new(RadioOptionId(30), "手动 Manual"),
            ],
            families,
            Some(RadioOptionId(10)),
            ActionId(3),
        );
        view.options[1].disabled = true;
        view.text_style.font_size = 16.0;
        view.text_style.line_height = 1.5;
        view.style.row_gap = 8.0;
        let tree = Tree::from_view(description(&view, 240.0), TreeLimits::default()).unwrap();
        let mut f = Self {
            tree,
            engine: Default::default(),
            text,
            view,
            width: 240.0,
        };
        f.prepare();
        f
    }
    fn prepare(&mut self) -> bool {
        self.tree
            .layout_with_text(&mut self.engine, &mut self.text, Size::new(300.0, 500.0))
            .unwrap()
    }
    fn update(&mut self) -> bool {
        self.tree
            .reconcile(description(&self.view, self.width))
            .unwrap();
        self.prepare()
    }
    fn input(&mut self, event: InputEvent) -> zui_core::DispatchResult {
        self.prepare();
        self.tree.dispatch_input(event).unwrap()
    }
    fn row_height(&mut self, index: usize) -> f64 {
        self.text
            .layout(TextRequest {
                text: &self.view.options[index].label,
                style: &self.view.text_style,
                width: TextWidth::AtMost(
                    self.width - 16.0 - self.view.style.indicator_size - self.view.style.label_gap,
                ),
            })
            .unwrap()
            .size
            .height
            .max(self.view.style.indicator_size as f64)
    }
    fn center(&mut self, index: usize) -> Point {
        let mut y = 8.0;
        for i in 0..index {
            y += self.row_height(i) + self.view.style.row_gap as f64;
        }
        Point::new(120.0, y + self.row_height(index) / 2.0)
    }
    fn click(&mut self, point: Point) -> zui_core::DispatchResult {
        self.input(mouse(true, point));
        self.input(mouse(false, point))
    }
}
fn description(view: &RadioGroupView, width: f32) -> View {
    View::new(view.clone()).keyed("radio").styled(LayoutStyle {
        width: Dimension::Length(width),
        padding: Edges::all(8.0),
        ..Default::default()
    })
}
fn mouse(pressed: bool, p: Point) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(p),
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
fn requested(result: zui_core::DispatchResult) -> RadioOptionId {
    assert_eq!(result.actions.len(), 1);
    assert_eq!(result.actions[0].action.id, ActionId(3));
    result.actions[0]
        .action
        .payload::<RadioChanged>()
        .unwrap()
        .selected
}
#[test]
fn padded_label_clicks_are_controlled_and_disabled_gaps_do_not_select() {
    let mut f = Fixture::new();
    let selected = f.center(0);
    let disabled = f.center(1);
    let other = f.center(2);
    assert!(f.click(selected).actions.is_empty());
    assert!(f.click(disabled).actions.is_empty());
    assert!(f.click(Point::new(2.0, other.y)).actions.is_empty());
    let gap_y = 8.0 + f.row_height(0) + 2.0;
    assert!(f.click(Point::new(other.x, gap_y)).actions.is_empty());
    for _ in 0..2 {
        assert_eq!(requested(f.click(other)), RadioOptionId(30));
    }
    assert_eq!(f.view.selected, Some(RadioOptionId(10)));
    f.view.selected = Some(RadioOptionId(30));
    assert!(!f.update());
    assert!(f.click(other).actions.is_empty());
    f.input(mouse(true, selected));
    assert!(f.input(mouse(false, other)).actions.is_empty());
    f.input(mouse(true, selected));
    f.input(InputEvent::PointerMoved(Point::new(290.0, 200.0)));
    assert_eq!(
        requested(f.input(mouse(false, selected))),
        RadioOptionId(10)
    );
}
#[test]
fn keyboard_navigation_skips_disabled_wraps_and_does_not_mutate_model() {
    let mut f = Fixture::new();
    f.input(key(Key::Tab, true));
    assert_eq!(f.tree.focused(), Some(f.tree.root()));
    assert_eq!(
        requested(f.input(key(Key::ArrowDown, true))),
        RadioOptionId(30)
    );
    assert!(f.input(key(Key::ArrowDown, true)).actions.is_empty()); // wraps to selected value
    assert_eq!(requested(f.input(key(Key::End, true))), RadioOptionId(30));
    f.input(key(Key::Space, true));
    assert_eq!(
        requested(f.input(key(Key::Space, false))),
        RadioOptionId(30)
    );
    assert!(f.input(key(Key::Home, true)).actions.is_empty());
    assert_eq!(
        requested(f.input(key(Key::ArrowLeft, true))),
        RadioOptionId(30)
    );
    assert!(!f.input(key(Key::Enter, true)).handled);
    let mut modified = key(Key::Home, true);
    if let InputEvent::Key(k) = &mut modified {
        k.modifiers.control = true;
    }
    assert!(!f.input(modified).handled);
    assert_eq!(f.view.selected, Some(RadioOptionId(10)));
    // The group is one runtime node and one Tab stop, regardless of option count.
    assert_eq!(f.tree.len(), 1);
    f.input(key(Key::Tab, true));
    assert_eq!(f.tree.focused(), Some(f.tree.root()));
}
#[test]
fn option_updates_escape_and_focus_loss_cancel_armed_selection() {
    let mut f = Fixture::new();
    let other = f.center(2);
    f.input(mouse(true, other));
    assert!(f.tree.pointer_capture().is_some());
    f.input(key(Key::Escape, true));
    assert_eq!(f.tree.pointer_capture(), None);
    assert!(f.input(mouse(false, other)).actions.is_empty());
    f.input(mouse(true, other));
    f.view.options.reverse();
    f.update();
    assert!(f.input(mouse(false, other)).actions.is_empty());
    f.input(key(Key::End, true));
    f.input(key(Key::Space, true));
    f.view.selected = Some(RadioOptionId(30));
    f.update();
    assert!(f.input(key(Key::Space, false)).actions.is_empty());
    f.input(key(Key::Space, true));
    f.tree.cancel_input();
    assert!(f.input(key(Key::Space, false)).actions.is_empty());
    f.view.disabled = true;
    f.update();
    assert_eq!(f.tree.focused(), None);
    assert!(f.click(other).actions.is_empty());
}
#[test]
fn wrapping_metrics_empty_group_and_invalid_identity_are_explicit() {
    let mut f = Fixture::new();
    f.view.options[0].label = "自动选择需要换行的中文标签 Auto mode with a long label".into();
    for width in [240.0, 150.0] {
        f.width = width;
        f.update();
        let expected =
            16.0 + (0..3).map(|i| f.row_height(i)).sum::<f64>() + 2.0 * f.view.style.row_gap as f64;
        let actual = f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height;
        assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
    }
    f.view.options.clear();
    f.view.selected = None;
    f.update();
    assert!(!f.input(key(Key::Tab, true)).handled);
    assert_eq!(
        f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height,
        16.0
    );
    for options in [
        vec![
            RadioOption::new(RadioOptionId(1), "one"),
            RadioOption::new(RadioOptionId(1), "duplicate"),
        ],
        vec![],
    ] {
        f.view.options = options;
        f.view.selected = Some(RadioOptionId(1));
        f.tree.reconcile(description(&f.view, f.width)).unwrap();
        assert!(
            f.tree
                .layout_with_text(&mut f.engine, &mut f.text, Size::new(300.0, 500.0))
                .is_err()
        );
        assert!(f.tree.scene().is_err());
    }
}

#[test]
fn invalid_selection_and_metrics_cannot_bypass_prepare_on_property_updates() {
    let mut f = Fixture::new();
    f.tree.scene().unwrap();
    f.view.selected = Some(RadioOptionId(999));
    f.tree.reconcile(description(&f.view, f.width)).unwrap();
    assert!(
        f.tree
            .layout_with_text(&mut f.engine, &mut f.text, Size::new(300.0, 500.0))
            .is_err()
    );
    assert!(f.tree.scene().is_err());
    f.view.selected = None;
    f.update();
    f.view.style.row_gap = -1.0;
    f.tree.reconcile(description(&f.view, f.width)).unwrap();
    assert!(
        f.tree
            .layout_with_text(&mut f.engine, &mut f.text, Size::new(300.0, 500.0))
            .is_err()
    );
    f.view.style.row_gap = 8.0;
    for option in &mut f.view.options {
        option.disabled = true;
    }
    f.update();
    assert!(!f.input(key(Key::Tab, true)).handled);
    assert!(f.tree.focused().is_none());
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    for selected in [10, 20, 30] {
        f.view.selected = Some(RadioOptionId(selected));
        f.update();
        let centers: Vec<_> = (0..3).map(|i| f.center(i)).collect();
        let scene = f.tree.scene().unwrap();
        for scale in [1.0, 2.0] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new((260.0 * scale) as u32, (200.0 * scale) as u32),
                        scale,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for (i, p) in centers.iter().enumerate() {
                let colors = if i == 1 {
                    f.view.style.disabled
                } else {
                    f.view.style.normal
                };
                let expected = if f.view.options[i].id.0 == selected {
                    colors.dot
                } else {
                    colors.background
                };
                let x = (18.0 * scale) as usize;
                let y = (p.y * scale) as usize;
                let offset = y * pixels.stride() + x * 4;
                assert_eq!(
                    &pixels.pixels()[offset..offset + 4],
                    &[expected.r, expected.g, expected.b, expected.a]
                );
            }
            assert_eq!(&pixels.pixels()[..4], &[0, 0, 0, 255]);
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn radio_selected_and_disabled_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn radio_selected_and_disabled_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

fn horizontal_slot(f: &Fixture) -> f64 {
    ((f.width as f64 - 16.0) - (f.view.options.len() - 1) as f64 * f.view.style.row_gap as f64)
        .max(0.0)
        / f.view.options.len() as f64
}
fn horizontal_center(f: &Fixture, index: usize) -> Point {
    Point::new(
        8.0 + index as f64 * (horizontal_slot(f) + f.view.style.row_gap as f64)
            + horizontal_slot(f) / 2.0,
        f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height / 2.0,
    )
}
#[test]
fn horizontal_slots_share_height_wrap_labels_and_use_padding_aware_hit_geometry() {
    let mut f = Fixture::new();
    f.view.orientation = zui_widgets::RadioOrientation::Horizontal;
    f.view.options[0].label = "自动选择模式 Auto mode with wrapping".into();
    for width in [600.0, 240.0] {
        f.width = width;
        f.update();
        let label_width = (horizontal_slot(&f)
            - f.view.style.indicator_size as f64
            - f.view.style.label_gap as f64)
            .max(0.0) as f32;
        let expected = f
            .view
            .options
            .iter()
            .map(|option| {
                f.text
                    .layout(TextRequest {
                        text: &option.label,
                        style: &f.view.text_style,
                        width: TextWidth::AtMost(label_width),
                    })
                    .unwrap()
                    .size
                    .height
            })
            .fold(f.view.style.indicator_size as f64, f64::max)
            + 16.0;
        assert!(
            (f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height - expected).abs() < 0.02
        );
        let point = horizontal_center(&f, 2);
        assert_eq!(requested(f.click(point)), RadioOptionId(30));
        let point = horizontal_center(&f, 1);
        assert!(f.click(point).actions.is_empty());
        let gap = Point::new(
            8.0 + horizontal_slot(&f) + f.view.style.row_gap as f64 / 2.0,
            expected / 2.0,
        );
        assert!(f.click(gap).actions.is_empty());
        assert!(f.click(Point::new(2.0, expected / 2.0)).actions.is_empty());
    }
    f.width = 20.0;
    f.update();
    assert!(f.click(Point::new(9.0, 18.0)).actions.is_empty());
}
#[test]
fn orientation_updates_cancel_press_and_preserve_identity_and_keyboard_selection_rules() {
    let mut f = Fixture::new();
    let id = f.tree.root();
    let point = f.center(2);
    f.input(mouse(true, point));
    f.view.orientation = zui_widgets::RadioOrientation::Horizontal;
    f.width = 600.0;
    assert!(f.update());
    assert_eq!(f.tree.root(), id);
    let point = horizontal_center(&f, 2);
    assert!(f.input(mouse(false, point)).actions.is_empty());
    f.tree.cancel_input();
    f.input(key(Key::Tab, true));
    assert_eq!(
        requested(f.input(key(Key::ArrowRight, true))),
        RadioOptionId(30)
    );
    assert!(f.input(key(Key::ArrowLeft, true)).actions.is_empty()); // selected 10, disabled 20 skipped
    f.view.options.swap(0, 2);
    f.update();
    let point = horizontal_center(&f, 0);
    assert_eq!(requested(f.click(point)), RadioOptionId(30));
    assert_eq!(f.tree.root(), id);
    f.view.options.clear();
    f.view.selected = None;
    f.update();
    assert!(!f.input(key(Key::Tab, true)).handled);
    assert_eq!(f.tree.layout_box(id).unwrap().bounds.size.height, 16.0);
}
fn verify_horizontal(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    f.view.orientation = zui_widgets::RadioOrientation::Horizontal;
    f.width = 600.0;
    f.view.selected = Some(RadioOptionId(30));
    f.update();
    let scene = f.tree.scene().unwrap();
    let center = horizontal_center(&f, 2);
    let dot_x = 8.0
        + 2.0 * (horizontal_slot(&f) + f.view.style.row_gap as f64)
        + f.view.style.indicator_size as f64 / 2.0;
    for scale in [1.0, 2.0] {
        let pixels = renderer
            .render(
                &scene,
                Frame {
                    extent: Extent::new((640.0 * scale) as u32, (100.0 * scale) as u32),
                    scale,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let sample = |x: f64, y: f64| {
            let i = (y * scale) as usize * pixels.stride() + (x * scale) as usize * 4;
            &pixels.pixels()[i..i + 4]
        };
        let color = f.view.style.normal.dot;
        assert_eq!(
            sample(dot_x, center.y),
            &[color.r, color.g, color.b, color.a]
        );
        let background = f.view.style.normal.background;
        assert_eq!(
            sample(18.0, center.y),
            &[background.r, background.g, background.b, background.a]
        );
        assert_eq!(
            sample(8.0 + horizontal_slot(&f) + 4.0, center.y),
            &[0, 0, 0, 255]
        );
        assert_eq!(sample(602.0, center.y), &[0, 0, 0, 255]);
    }
}
#[cfg(feature = "cpu")]
#[test]
fn horizontal_radio_rows_render_cpu() {
    verify_horizontal(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter"]
fn horizontal_radio_rows_render_gpu() {
    verify_horizontal(
        &mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap(),
    );
}

#[cfg(feature = "cpu")]
#[test]
fn mouse_hides_row_focus_and_keyboard_restores_it() {
    let mut f = Fixture::new();
    // A unique color lets us inspect only the rectangular focus indicator.
    f.view.style.focus_ring = Color::rgb(255, 0, 255);
    f.update();
    let has_ring = |f: &mut Fixture| {
        let pixels = zui_render_cpu::CpuRenderer::default()
            .render(
                &f.tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(260, 200),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        pixels
            .pixels()
            .chunks_exact(4)
            .any(|p| p[0] > 80 && p[1] == 0 && p[2] > 80)
    };
    let point = f.center(0);
    f.click(point);
    assert!(f.tree.focused().is_some());
    assert!(!has_ring(&mut f));
    // Rebuilding the controlled selection must not reintroduce the mouse ring.
    f.update();
    assert!(!has_ring(&mut f));
    f.input(key(Key::ArrowDown, true));
    assert!(has_ring(&mut f));
    let point = f.center(2);
    f.click(point);
    assert!(!has_ring(&mut f));
    f.tree.request_focus(None).unwrap();
    f.input(key(Key::Tab, true));
    assert!(f.tree.focused().is_some());
    assert!(has_ring(&mut f));
}
