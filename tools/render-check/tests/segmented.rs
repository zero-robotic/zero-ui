use std::sync::{Arc, OnceLock};
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    text::{TextRequest, TextService, TextWidth},
    view::View,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{SegmentChanged, SegmentId, SegmentOption, SegmentedControlView};
struct Fixture {
    tree: Tree,
    text: zui_text::ParleyText,
    view: SegmentedControlView,
    width: f32,
}

#[cfg(feature = "cpu")]
#[test]
fn joined_segments_round_only_outer_corners_and_draw_each_seam_once() {
    verify_joined(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter"]
fn joined_segments_round_only_outer_corners_and_draw_each_seam_once_gpu() {
    verify_joined(
        &mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap(),
    );
}

fn verify_joined(renderer: &mut dyn OffscreenRenderer) {
    for theme in [
        Theme::desktop_dark(),
        Theme::desktop_light(),
        Theme::dashboard(),
    ] {
        let mut fixture = Fixture::new();
        fixture.width = 316.0;
        fixture.view.style = zui_widgets::SegmentedStyle::from_theme(&theme);
        fixture.view.style.gap = 0.0;
        fixture.view.style.radius = 8.0;
        for option in &mut fixture.view.options {
            option.label.clear();
            option.disabled = false;
        }
        fixture.update();
        for disabled in [false, true] {
            fixture.view.disabled = disabled;
            fixture.update();
            let colors = if disabled {
                fixture.view.style.disabled
            } else {
                fixture.view.style.normal
            };
            for scale in [1_u32, 2] {
                let background = Color::rgb(217, 17, 217);
                let pixels = renderer
                    .render(
                        &fixture.tree.scene().unwrap(),
                        Frame {
                            extent: Extent::new(320 * scale, 100 * scale),
                            scale: f64::from(scale),
                            background,
                        },
                    )
                    .unwrap();
                for (horizontal, vertical, expected) in [
                    (8, 8, background),
                    (307, 8, background),
                    (107, 10, colors.selected_background),
                    (108, 10, colors.border),
                    (109, 10, colors.background),
                    (207, 10, colors.background),
                    (208, 10, colors.border),
                    (209, 10, colors.background),
                ] {
                    let offset = (vertical * scale) as usize * pixels.stride()
                        + (horizontal * scale * 4) as usize;
                    assert_eq!(
                        &pixels.pixels()[offset..offset + 4],
                        &[expected.r, expected.g, expected.b, expected.a],
                        "pixel ({horizontal}, {vertical}), scale {scale}, disabled {disabled}"
                    );
                }
            }
        }
    }
}
impl Fixture {
    fn new() -> Self {
        static FONT: OnceLock<Arc<[u8]>> = OnceLock::new();
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(
                FONT.get_or_init(|| {
                    Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice())
                })
                .clone(),
            )
            .unwrap();
        let mut view = SegmentedControlView::themed(
            &Theme::dashboard(),
            vec![
                SegmentOption::new(SegmentId(10), "概览"),
                SegmentOption::new(SegmentId(20), "维护"),
                SegmentOption::new(SegmentId(30), "趋势"),
            ],
            families,
            Some(SegmentId(10)),
            ActionId(8),
        );
        view.options[1].disabled = true;
        let tree = Tree::from_view(description(&view, 320.0), Default::default()).unwrap();
        let mut f = Self {
            tree,
            text,
            view,
            width: 320.0,
        };
        f.prepare();
        f
    }
    fn prepare(&mut self) -> bool {
        self.tree
            .layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut self.text,
                Size::new(640.0, 400.0),
            )
            .unwrap()
    }
    fn update(&mut self) -> bool {
        self.tree
            .reconcile(description(&self.view, self.width))
            .unwrap();
        self.prepare()
    }
    fn slot(&self) -> f64 {
        ((self.width as f64 - 16.0)
            - (self.view.options.len() - 1) as f64 * self.view.style.gap as f64)
            .max(0.0)
            / self.view.options.len() as f64
    }
    fn center(&self, index: usize) -> Point {
        Point::new(
            8.0 + index as f64 * (self.slot() + self.view.style.gap as f64) + self.slot() / 2.0,
            self.tree
                .layout_box(self.tree.root())
                .unwrap()
                .bounds
                .size
                .height
                / 2.0,
        )
    }
    fn input(&mut self, input: InputEvent) -> zui_core::DispatchResult {
        self.prepare();
        self.tree.dispatch_input(input).unwrap()
    }
    fn click(&mut self, p: Point) -> zui_core::DispatchResult {
        self.input(mouse(true, p));
        self.input(mouse(false, p))
    }
}
fn description(view: &SegmentedControlView, width: f32) -> View {
    View::new(view.clone())
        .keyed("segments")
        .styled(LayoutStyle {
            width: Dimension::Length(width),
            padding: Edges::all(8.0),
            ..Default::default()
        })
}
fn mouse(pressed: bool, position: Point) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(position),
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
fn requested(result: zui_core::DispatchResult) -> SegmentId {
    assert_eq!(result.actions.len(), 1);
    assert_eq!(result.actions[0].action.id, ActionId(8));
    result.actions[0]
        .action
        .payload::<SegmentChanged>()
        .unwrap()
        .selected
}
#[test]
fn segment_requests_are_controlled_and_hit_testing_excludes_gaps_padding_and_disabled_cells() {
    let mut f = Fixture::new();
    let first = f.center(0);
    let last = f.center(2);
    let disabled = f.center(1);
    assert!(f.click(first).actions.is_empty());
    assert!(f.click(disabled).actions.is_empty());
    assert_eq!(requested(f.click(last)), SegmentId(30));
    assert_eq!(requested(f.click(last)), SegmentId(30));
    assert_eq!(f.view.selected, Some(SegmentId(10)));
    let gap = Point::new(8.0 + f.slot() + f.view.style.gap as f64 / 2.0, first.y);
    assert!(f.click(gap).actions.is_empty());
    assert!(f.click(Point::new(2.0, first.y)).actions.is_empty());
    f.input(mouse(true, first));
    assert_eq!(f.tree.pointer_capture(), Some(f.tree.root()));
    assert!(f.input(mouse(false, last)).actions.is_empty());
    assert_eq!(f.tree.pointer_capture(), None);
    f.view.selected = Some(SegmentId(30));
    assert!(!f.update());
    assert!(f.click(last).actions.is_empty());
}

#[test]
fn changing_only_corner_radius_repaints_without_relayout() {
    let mut fixture = Fixture::new();
    let before = fixture.tree.scene().unwrap();
    fixture.view.style.radius = 13.0;
    assert!(!fixture.update());
    let after = fixture.tree.scene().unwrap();
    assert_ne!(before.commands(), after.commands());
}
#[test]
fn keyboard_reorder_and_disabled_updates_keep_one_focus_owner_and_cancel_stale_gestures() {
    let mut f = Fixture::new();
    let id = f.tree.root();
    f.input(key(Key::Tab, true));
    assert_eq!(f.tree.focused(), Some(id));
    assert_eq!(
        requested(f.input(key(Key::ArrowRight, true))),
        SegmentId(30)
    );
    f.input(key(Key::Space, true));
    assert_eq!(requested(f.input(key(Key::Space, false))), SegmentId(30));
    assert!(f.input(key(Key::Home, true)).actions.is_empty());
    let p = f.center(2);
    f.input(mouse(true, p));
    f.view.options.reverse();
    f.update();
    assert!(f.input(mouse(false, p)).actions.is_empty());
    assert_eq!(f.tree.root(), id);
    assert_eq!(f.tree.len(), 1);
    f.input(key(Key::End, true));
    f.input(key(Key::Space, true));
    f.input(key(Key::Escape, true));
    assert!(f.input(key(Key::Space, false)).actions.is_empty());
    f.view.disabled = true;
    f.update();
    assert_eq!(f.tree.focused(), None);
    let p = f.center(0);
    assert!(f.click(p).actions.is_empty());
}
#[test]
fn label_wrapping_common_height_empty_and_invalid_configuration_are_explicit() {
    let mut f = Fixture::new();
    f.view.options[0].label = "需要换行的长标签 Segmented control".into();
    for width in [600.0, 240.0] {
        f.width = width;
        f.update();
        let label_width = (f.slot() - f.view.style.padding_x as f64 * 2.0).max(0.0) as f32;
        let height = f
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
            .fold(0.0, f64::max);
        let expected = (height + f.view.style.padding_y as f64 * 2.0)
            .max(f.view.style.min_height as f64)
            + 16.0;
        assert!(
            (f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height - expected).abs() < 0.02
        );
    }
    f.width = 20.0;
    f.update();
    assert!(f.click(Point::new(9.0, 20.0)).actions.is_empty());
    f.view.options.clear();
    f.view.selected = None;
    f.update();
    assert_eq!(
        f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height,
        16.0
    );
    assert!(!f.input(key(Key::Tab, true)).handled);
    for variant in 0..7 {
        let mut bad = Fixture::new();
        match variant {
            0 => bad.view.options[1].id = SegmentId(10),
            1 => bad.view.selected = Some(SegmentId(999)),
            2 => bad.view.style.padding_x = f32::INFINITY,
            3 => bad.view.style.gap = -1.0,
            4 => bad.view.style.radius = -1.0,
            5 => bad.view.style.radius = f32::NAN,
            _ => bad.view.style.radius = f32::INFINITY,
        }
        bad.tree
            .reconcile(description(&bad.view, bad.width))
            .unwrap();
        assert!(
            bad.tree
                .layout_with_text(
                    &mut zui_layout::TaffyLayout::default(),
                    &mut bad.text,
                    Size::new(640.0, 400.0)
                )
                .is_err()
        );
        assert!(bad.tree.scene().is_err());
    }
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    f.view.style.normal.selected_background = Color::rgb(20, 120, 210);
    f.view.style.normal.background = Color::rgb(20, 30, 40);
    f.view.style.disabled.background = Color::rgb(50, 60, 70);
    f.update();
    for selected in [SegmentId(10), SegmentId(30)] {
        f.view.selected = Some(selected);
        f.update();
        let scene = f.tree.scene().unwrap();
        for scale in [1.0, 2.0] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new((340.0 * scale) as u32, (100.0 * scale) as u32),
                        scale,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let sample = |x: f64, y: f64| {
                let i = (y * scale) as usize * pixels.stride() + (x * scale) as usize * 4;
                &pixels.pixels()[i..i + 4]
            };
            for i in 0..3 {
                let color = if i == 1 {
                    Color::rgb(50, 60, 70)
                } else if f.view.options[i].id == selected {
                    Color::rgb(20, 120, 210)
                } else {
                    Color::rgb(20, 30, 40)
                };
                assert_eq!(
                    sample(12.0 + i as f64 * (f.slot() + f.view.style.gap as f64), 12.0),
                    &[color.r, color.g, color.b, color.a]
                );
            }
            assert_eq!(sample(8.0 + f.slot() + 1.0, 12.0), &[0, 0, 0, 255]);
            assert_eq!(sample(2.0, 12.0), &[0, 0, 0, 255]);
            assert_eq!(sample(322.0, 12.0), &[0, 0, 0, 255]);
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn segmented_selection_disabled_and_spacing_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter"]
fn segmented_selection_disabled_and_spacing_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
