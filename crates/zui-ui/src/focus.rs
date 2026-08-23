use crate::WidgetId;

#[derive(Default)]
pub struct FocusManager {
    focused: Option<WidgetId>,
}

impl FocusManager {
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }
    pub fn request_focus(&mut self, id: WidgetId) {
        self.focused = Some(id);
    }
    pub fn clear(&mut self) {
        self.focused = None;
    }
    pub fn has_focus(&self, id: WidgetId) -> bool {
        self.focused == Some(id)
    }
}
