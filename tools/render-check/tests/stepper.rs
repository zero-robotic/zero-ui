use std::sync::Arc;
use zui_api::{action::ActionId, event::*, view::Key as ViewKey};
use zui_core::Tree;
use zui_theme::Theme;
use zui_types::{NodeId, Point, Size};
use zui_widgets::{NumericRange, StepperChanged, StepperView};
struct Fixture {
    tree: Tree,
    text: zui_text::ParleyText,
    view: StepperView,
}
impl Fixture {
    fn new() -> Self {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let view = StepperView::themed(
            &Theme::dashboard(),
            families,
            3.0,
            NumericRange::new(0.0, 10.0, 3.0).unwrap(),
            ActionId(19),
        );
        let tree = Tree::from_view(view.clone().build().unwrap(), Default::default()).unwrap();
        let mut f = Self { tree, text, view };
        f.update();
        f
    }
    fn update(&mut self) {
        self.tree
            .reconcile(self.view.clone().build().unwrap())
            .unwrap();
        self.tree
            .layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut self.text,
                Size::new(240.0, 60.0),
            )
            .unwrap();
    }
    fn child(&self, key: &str) -> NodeId {
        self.tree
            .children(self.tree.root())
            .unwrap()
            .iter()
            .copied()
            .find(|id| self.tree.key(*id).unwrap() == Some(&ViewKey::from(key)))
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
    fn mouse(&mut self, pressed: bool) -> zui_core::DispatchResult {
        let bounds = self
            .tree
            .layout_box(self.child("increment"))
            .unwrap()
            .bounds;
        self.tree
            .dispatch_input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed,
                position: Some(Point::new(bounds.origin.x + 4.0, bounds.origin.y + 4.0)),
            })
            .unwrap()
    }
}
fn requested(result: zui_core::DispatchResult, value: f64) {
    assert_eq!(result.actions.len(), 1);
    assert_eq!(result.actions[0].action.id, ActionId(19));
    assert_eq!(
        result.actions[0].action.payload::<StepperChanged>(),
        Some(&StepperChanged { value })
    );
}
#[test]
fn stepping_is_controlled_and_supports_off_grid_values_and_short_final_interval() {
    let mut f = Fixture::new();
    let inc = f.child("increment");
    for _ in 0..2 {
        f.mouse(true);
        requested(f.mouse(false), 6.0);
    }
    assert_eq!(f.view.value, 3.0);
    assert_eq!(f.tree.focused(), Some(inc));
    f.view.value = 9.0;
    f.update();
    assert_eq!(f.child("increment"), inc);
    requested(f.key(Key::Enter, true), 10.0);
    f.view.value = 10.0;
    f.update();
    assert!(f.tree.focused().is_none());
    f.mouse(true);
    assert!(f.mouse(false).actions.is_empty());
    f.tree.request_focus(Some(f.child("decrement"))).unwrap();
    requested(f.key(Key::Enter, true), 9.0);
    f.view.value = 4.0;
    f.update();
    requested(f.key(Key::Enter, true), 3.0);
    f.tree.request_focus(Some(inc)).unwrap();
    requested(f.key(Key::Enter, true), 6.0);
    f.view.value = 0.0;
    f.update();
    assert!(f.tree.request_focus(Some(f.child("decrement"))).is_err());
    f.view.disabled = true;
    f.update();
    assert!(f.tree.request_focus(Some(inc)).is_err());
}
#[test]
fn changed_values_ranges_and_bindings_cancel_pending_activation_but_formatting_keeps_identity() {
    let mut f = Fixture::new();
    let inc = f.child("increment");
    f.tree.request_focus(Some(inc)).unwrap();
    f.key(Key::Space, true);
    f.view.value = 4.0;
    f.update(); // Same next value (6), but a different source.
    assert!(f.key(Key::Space, false).actions.is_empty());
    f.key(Key::Space, true);
    f.view.range = NumericRange::new(0.0, 12.0, 3.0).unwrap();
    f.update();
    assert!(f.key(Key::Space, false).actions.is_empty());
    f.mouse(true);
    f.view.action = ActionId(20);
    f.update();
    assert!(f.mouse(false).actions.is_empty());
    f.view.action = ActionId(19);
    f.view.display = Some("4 台设备".into());
    f.update();
    assert_eq!(f.child("increment"), inc);
    assert_eq!(f.tree.focused(), Some(inc));
    f.tree.scene().unwrap();
    requested(f.key(Key::Enter, true), 6.0);
}
#[test]
fn invalid_values_are_rejected_before_building_children() {
    let mut view = StepperView::themed(
        &Theme::dashboard(),
        vec![],
        0.0,
        NumericRange::new(-2.0, 2.0, 0.5).unwrap(),
        ActionId(1),
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -3.0, 3.0] {
        view.value = value;
        assert!(view.clone().build().is_err());
    }
    view.value = -1.25;
    assert!(view.build().is_ok());
}
