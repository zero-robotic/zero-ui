use std::sync::Arc;
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{ToggleButtonView, ToggleChanged};
struct Fixture {
    tree: Tree,
    text: zui_text::ParleyText,
    view: ToggleButtonView,
}
fn description(view: &ToggleButtonView) -> View {
    View::new(view.clone()).styled(LayoutStyle {
        width: Dimension::Length(160.0),
        height: Dimension::Length(48.0),
        padding: Edges::all(8.0),
        ..Default::default()
    })
}
impl Fixture {
    fn new() -> Self {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let view = ToggleButtonView::themed(
            &Theme::dashboard(),
            "显示网格",
            families,
            false,
            ActionId(4),
        );
        let tree = Tree::from_view(description(&view), Default::default()).unwrap();
        let mut f = Self { tree, text, view };
        f.layout();
        f
    }
    fn layout(&mut self) {
        self.tree
            .layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut self.text,
                Size::new(180.0, 60.0),
            )
            .unwrap();
    }
    fn update(&mut self) {
        self.tree.reconcile(description(&self.view)).unwrap();
        self.layout();
    }
    fn mouse(&mut self, pressed: bool) -> zui_core::DispatchResult {
        self.tree
            .dispatch_input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed,
                position: Some(Point::new(30.0, 24.0)),
            })
            .unwrap()
    }
    fn key(&mut self, key: Key, pressed: bool) -> zui_core::DispatchResult {
        self.tree
            .dispatch_input(InputEvent::Key(KeyInput {
                key,
                pressed,
                repeat: false,
                text: None,
                modifiers: Default::default(),
            }))
            .unwrap()
    }
}
#[test]
fn controlled_mouse_and_keyboard_requests_and_selection_change_cancel_armed_gestures() {
    let mut f = Fixture::new();
    let id = f.tree.root();
    for _ in 0..2 {
        f.mouse(true);
        let result = f.mouse(false);
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].action.id, ActionId(4));
        assert_eq!(
            result.actions[0].action.payload::<ToggleChanged>(),
            Some(&ToggleChanged { selected: true })
        );
        assert!(!f.view.selected);
    }
    f.view.selected = true;
    f.update();
    assert_eq!(f.tree.focused(), Some(id));
    let result = f.key(Key::Enter, true);
    assert_eq!(
        result.actions[0].action.payload::<ToggleChanged>(),
        Some(&ToggleChanged { selected: false })
    );
    f.key(Key::Space, true);
    f.view.selected = false;
    f.update();
    assert!(f.key(Key::Space, false).actions.is_empty());
    f.mouse(true);
    f.view.selected = true;
    f.update();
    assert!(f.mouse(false).actions.is_empty());
    f.key(Key::Space, true);
    f.view.action = ActionId(9);
    f.update();
    assert!(f.key(Key::Space, false).actions.is_empty());
    f.view.disabled = true;
    f.update();
    assert!(f.tree.focused().is_none());
    f.mouse(true);
    assert!(f.mouse(false).actions.is_empty());
}
#[test]
fn canceled_pointer_and_outside_release_do_not_toggle() {
    let mut f = Fixture::new();
    f.mouse(true);
    let result = f
        .tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(175.0, 55.0)),
        })
        .unwrap();
    assert!(result.actions.is_empty());
    f.mouse(true);
    f.tree.cancel_input();
    assert!(f.mouse(false).actions.is_empty());
}
#[cfg(feature = "cpu")]
#[test]
fn selected_unselected_and_disabled_styles_render_at_both_scales() {
    let mut f = Fixture::new();
    let mut renderer = zui_render_cpu::CpuRenderer::default();
    for selected in [false, true] {
        for disabled in [false, true] {
            f.view.selected = selected;
            f.view.disabled = disabled;
            f.update();
            let style = if selected {
                f.view.style.selected
            } else {
                f.view.style.unselected
            };
            let color = if disabled {
                style.disabled.background
            } else {
                style.normal.background
            };
            for scale in [1u32, 2] {
                let pixels = renderer
                    .render(
                        &f.tree.scene().unwrap(),
                        Frame {
                            extent: Extent::new(180 * scale, 60 * scale),
                            scale: scale as f64,
                            background: Color::BLACK,
                        },
                    )
                    .unwrap();
                let offset = (4 * scale) as usize * pixels.stride() + (4 * scale * 4) as usize;
                assert_eq!(
                    &pixels.pixels()[offset..offset + 4],
                    &[color.r, color.g, color.b, color.a]
                );
            }
        }
    }
}
