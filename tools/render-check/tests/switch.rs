use std::time::Duration;
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{SwitchChanged, SwitchView};
fn view(properties: SwitchView) -> View {
    View::new(properties).styled(LayoutStyle {
        width: Dimension::Length(60.0),
        height: Dimension::Length(38.0),
        padding: Edges::all(6.0),
        ..Default::default()
    })
}
fn layout(tree: &mut Tree) {
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(80.0, 50.0),
    )
    .unwrap();
}
fn update(tree: &mut Tree, properties: SwitchView) {
    tree.reconcile(view(properties)).unwrap();
    layout(tree);
}
fn properties() -> SwitchView {
    SwitchView::themed(&Theme::dark(), false, ActionId(9))
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
fn button(pressed: bool, x: f64) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(x, 19.0)),
    }
}
#[test]
fn controlled_switch_supports_pointer_space_cancel_and_disabled_without_optimistic_value_changes() {
    let mut p = properties();
    let mut tree = Tree::from_view(view(p), Default::default()).unwrap();
    layout(&mut tree);
    tree.dispatch_input(button(true, 20.0)).unwrap();
    assert_eq!(tree.pointer_capture(), Some(tree.root()));
    let changed = tree.dispatch_input(button(false, 20.0)).unwrap();
    assert_eq!(
        changed.actions[0].action.payload::<SwitchChanged>(),
        Some(&SwitchChanged { checked: true })
    );
    assert_eq!(tree.pointer_capture(), None);
    tree.dispatch_input(key(Key::Space, true, false)).unwrap();
    assert!(
        tree.dispatch_input(key(Key::Space, true, true))
            .unwrap()
            .actions
            .is_empty()
    );
    let again = tree.dispatch_input(key(Key::Space, false, false)).unwrap();
    assert_eq!(
        again.actions[0].action.payload::<SwitchChanged>(),
        Some(&SwitchChanged { checked: true })
    );
    p.checked = true;
    update(&mut tree, p);
    tree.dispatch_input(key(Key::Space, true, false)).unwrap();
    let off = tree.dispatch_input(key(Key::Space, false, false)).unwrap();
    assert_eq!(
        off.actions[0].action.payload::<SwitchChanged>(),
        Some(&SwitchChanged { checked: false })
    );
    tree.dispatch_input(button(true, 20.0)).unwrap();
    assert!(
        tree.dispatch_input(button(false, 75.0))
            .unwrap()
            .actions
            .is_empty()
    );
    tree.dispatch_input(key(Key::Space, true, false)).unwrap();
    tree.dispatch_input(key(Key::Escape, true, false)).unwrap();
    assert!(
        tree.dispatch_input(key(Key::Space, false, false))
            .unwrap()
            .actions
            .is_empty()
    );
    assert!(
        tree.dispatch_input(key(Key::Enter, true, false))
            .unwrap()
            .actions
            .is_empty()
    );
    p.disabled = true;
    update(&mut tree, p);
    assert!(tree.focused().is_none());
    tree.dispatch_input(button(true, 20.0)).unwrap();
    assert!(
        tree.dispatch_input(button(false, 20.0))
            .unwrap()
            .actions
            .is_empty()
    );
    assert!(
        tree.advance_animations(Duration::from_secs(1))
            .unwrap()
            .next_frame
            .is_none()
    );
}
#[test]
fn confirmed_transition_reverses_from_current_position_and_reduced_motion_stops_scheduling() {
    let mut p = properties();
    let mut tree = Tree::from_view(view(p), Default::default()).unwrap();
    layout(&mut tree);
    assert!(
        tree.advance_animations(Duration::ZERO)
            .unwrap()
            .next_frame
            .is_none()
    );
    let off = tree.scene().unwrap();
    p.checked = true;
    update(&mut tree, p);
    assert!(
        tree.advance_animations(Duration::ZERO)
            .unwrap()
            .next_frame
            .is_some()
    );
    assert_eq!(
        tree.advance_animations(Duration::from_millis(60))
            .unwrap()
            .invalidation,
        Invalidation::Paint
    );
    let midpoint = tree.scene().unwrap();
    p.checked = false;
    update(&mut tree, p);
    tree.advance_animations(Duration::from_millis(60)).unwrap();
    assert!(
        tree.advance_animations(Duration::from_millis(180))
            .unwrap()
            .next_frame
            .is_none()
    );
    assert_eq!(tree.scene().unwrap().commands(), off.commands());
    assert_ne!(midpoint.commands(), off.commands());
    p.checked = true;
    update(&mut tree, p);
    tree.advance_animations(Duration::from_millis(200)).unwrap();
    p.transition = None;
    update(&mut tree, p);
    assert!(
        tree.advance_animations(Duration::from_millis(210))
            .unwrap()
            .next_frame
            .is_none()
    );
    let snapped = tree.scene().unwrap();
    let mut fresh = Tree::from_view(view(p), Default::default()).unwrap();
    layout(&mut fresh);
    assert_eq!(snapped.commands(), fresh.scene().unwrap().commands());
}
#[test]
fn invalid_dimensions_and_zero_duration_fail_prepare_and_can_be_corrected() {
    for variant in 0..4 {
        let mut p = properties();
        match variant {
            0 => p.style.width = 10.0,
            1 => p.style.inset = 13.0,
            2 => p.style.height = f32::NAN,
            _ => p.transition = Some(Duration::ZERO),
        }
        let mut tree = Tree::from_view(view(p), Default::default()).unwrap();
        assert!(
            tree.layout(
                &mut zui_layout::TaffyLayout::default(),
                Size::new(80.0, 50.0)
            )
            .is_err()
        );
        assert!(tree.scene().is_err());
        update(&mut tree, properties());
        tree.scene().unwrap();
    }
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    for checked in [false, true] {
        let mut p = properties();
        p.checked = checked;
        p.transition = None;
        p.style.normal.off_track = Color::rgb(0, 0, 180);
        p.style.normal.on_track = Color::rgb(200, 0, 0);
        p.style.normal.off_thumb = Color::WHITE;
        p.style.normal.on_thumb = Color::WHITE;
        let mut tree = Tree::from_view(view(p), Default::default()).unwrap();
        layout(&mut tree);
        let scene = tree.scene().unwrap();
        for scale in [1.0, 2.0] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new((80.0 * scale) as u32, (50.0 * scale) as u32),
                        scale,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let sample = |x: usize, y: usize| {
                let i = y * scale as usize * pixels.stride() + x * scale as usize * 4;
                &pixels.pixels()[i..i + 4]
            };
            assert_eq!(
                sample(if checked { 41 } else { 19 }, 19),
                &[255, 255, 255, 255]
            );
            assert_eq!(
                sample(if checked { 19 } else { 41 }, 19),
                if checked {
                    &[200, 0, 0, 255]
                } else {
                    &[0, 0, 180, 255]
                }
            );
            assert_eq!(sample(6, 6), &[0, 0, 0, 255]); // rounded corner
            assert_eq!(sample(58, 19), &[0, 0, 0, 255]);
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn switch_track_and_thumb_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter"]
fn switch_track_and_thumb_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
