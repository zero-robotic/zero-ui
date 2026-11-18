//! Typed primary action/value contracts. No state runtime dependency.
use crate::{
    ButtonActivated, ButtonView, CheckBoxView, CheckChanged, CheckState, NumberChanged,
    NumberInputView, SliderChanged, SliderView,
};
use zui_api::action::{ActionId, ActionView, ValueView};

impl ActionView for ButtonView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = ButtonActivated;
    fn set_action(&mut self, action: ActionId) {
        self.action = action;
    }
}
impl ActionView for SliderView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = SliderChanged;
    fn set_action(&mut self, action: ActionId) {
        self.action = action;
    }
}
impl ValueView for SliderView {
    type Value = f64;
    fn set_value(&mut self, value: f64) {
        self.value = value;
    }
    fn requested_value(action: &SliderChanged) -> Option<f64> {
        Some(action.value)
    }
}
impl ActionView for NumberInputView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = NumberChanged;
    fn set_action(&mut self, action: ActionId) {
        self.editor.action = action;
    }
}
impl ValueView for NumberInputView {
    type Value = f64;
    fn set_value(&mut self, value: f64) {
        self.value = value;
    }
    fn requested_value(action: &NumberChanged) -> Option<f64> {
        Some(action.value)
    }
}
impl ActionView for CheckBoxView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = CheckChanged;
    fn set_action(&mut self, action: ActionId) {
        self.action = action;
    }
}
impl ValueView for CheckBoxView {
    type Value = CheckState;
    fn set_value(&mut self, value: CheckState) {
        self.state = value;
    }
    fn requested_value(action: &CheckChanged) -> Option<CheckState> {
        Some(action.state)
    }
}

impl ActionView for crate::SwitchView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = crate::SwitchChanged;
    fn set_action(&mut self, action: ActionId) {
        self.action = action;
    }
}
impl ValueView for crate::SwitchView {
    type Value = bool;
    fn set_value(&mut self, value: bool) {
        self.checked = value;
    }
    fn requested_value(action: &Self::Action) -> Option<bool> {
        Some(action.checked)
    }
}
impl ActionView for crate::ToggleButtonView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = crate::ToggleChanged;
    fn set_action(&mut self, action: ActionId) {
        self.action = action;
    }
}
impl ValueView for crate::ToggleButtonView {
    type Value = bool;
    fn set_value(&mut self, value: bool) {
        self.selected = value;
    }
    fn requested_value(action: &Self::Action) -> Option<bool> {
        Some(action.selected)
    }
}
impl ActionView for crate::RadioGroupView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = crate::RadioChanged;
    fn set_action(&mut self, action: ActionId) {
        self.action = action;
    }
}
impl ValueView for crate::RadioGroupView {
    type Value = Option<crate::RadioOptionId>;
    fn set_value(&mut self, value: Self::Value) {
        self.selected = value;
    }
    fn requested_value(action: &Self::Action) -> Option<Self::Value> {
        Some(Some(action.selected))
    }
}
impl ActionView for crate::TextInputView {
    fn into_view(self) -> zui_api::view::View {
        zui_api::view::View::new(self)
    }
    type Action = crate::TextInputEvent;
    fn set_action(&mut self, action: ActionId) {
        self.action = action;
    }
}
impl ValueView for crate::TextInputView {
    type Value = alloc::string::String;
    fn set_value(&mut self, value: Self::Value) {
        self.text = value;
    }
    fn requested_value(action: &Self::Action) -> Option<Self::Value> {
        match action {
            crate::TextInputEvent::Changed(text) => Some(text.clone()),
            _ => None,
        }
    }
}
