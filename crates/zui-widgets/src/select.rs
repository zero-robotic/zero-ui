//! Controlled non-editable dropdown. Mount its popup at the root overlay stack.
use crate::{
    ButtonView, ListBox, ListBoxView, ListItem, ListItemId, ListSelectionChanged, PopupPlacement,
    PopupView, action_button::ActionButtonView,
};
use alloc::{string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Edges, LayoutError, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::{Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectEvent {
    Open,
    Selected(ListItemId),
}
#[derive(Clone)]
pub struct SelectView {
    pub list: ListBoxView,
    pub button: ButtonView,
    pub placeholder: String,
}
impl SelectView {
    pub fn themed(
        theme: &Theme,
        items: Vec<ListItem>,
        selected: Option<ListItemId>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        let mut list = ListBoxView::themed(theme, items, families.clone(), selected, action);
        list.activation_only = true;
        Self {
            list,
            button: ButtonView::themed(theme, "", families, action),
            placeholder: "请选择".into(),
        }
    }
    pub fn build(&self) -> View {
        let mut button = self.button.clone();
        button.disabled |= self.list.disabled;
        button.label = self
            .list
            .selected
            .and_then(|id| self.list.items.iter().find(|i| i.id == id))
            .map_or_else(|| self.placeholder.clone(), |i| i.label.clone());
        button.trailing_icon = Some(crate::Icon::builtin(crate::IconKind::ChevronDown));
        View::new(ActionButtonView {
            button,
            payload: SelectEvent::Open,
        })
        .styled(LayoutStyle {
            padding: Edges::all(10.0),
            ..Default::default()
        })
    }
    pub fn popup(&self, anchor: Rect, height: f64, dismiss: ActionId) -> View {
        PopupView::new(
            PopupPlacement::Anchor(anchor),
            Size::new(anchor.size.width.max(160.0), height),
            dismiss,
        )
        .build(View::new(SelectList(self.list.clone())))
    }
}
#[derive(Clone)]
struct SelectList(ListBoxView);
struct SelectListWidget(ListBox);
impl ViewSpec for SelectList {
    type Widget = SelectListWidget;
    fn build(&self) -> Self::Widget {
        SelectListWidget(self.0.build())
    }
    fn update(&self, w: &mut Self::Widget) -> Invalidation {
        self.0.update(&mut w.0)
    }
}
impl Widget for SelectListWidget {
    fn enabled(&self) -> bool {
        self.0.enabled()
    }
    fn focusable(&self) -> bool {
        self.0.focusable()
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.0.measure(c)
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.0.prepare(c)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.0.paint(c, p)
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        let mut r = self.0.event(c, e);
        for a in &mut r.actions {
            if let Some(change) = a.payload::<ListSelectionChanged>() {
                *a = WidgetAction::new(a.id, SelectEvent::Selected(change.selected));
            }
        }
        r
    }
}
