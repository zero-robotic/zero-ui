//! Searchable dropdown; the application owns query, selection and popup lifetime.
use crate::{
    ButtonView, ListBox, ListBoxView, ListItem, ListItemId, ListMultiSelectionChanged,
    ListSelectionChanged, PopupPlacement, PopupView, TextInput, TextInputEvent, TextInputView,
    action_button::ActionButtonView, column,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use core::time::Duration;
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Dimension, Edges, LayoutError, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::{Rect, Size};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboBoxEvent {
    Open,
    QueryChanged(String),
    Submitted(String),
    Selected(ListItemId),
    SelectionChanged(BTreeSet<ListItemId>),
}
#[derive(Clone)]
pub struct ComboBoxView {
    pub list: ListBoxView,
    pub button: ButtonView,
    pub editor: TextInputView,
    pub placeholder: String,
    pub query: String,
    pub searchable: bool,
    /// Enter in the editor can submit arbitrary text. Selection-only by default.
    pub allow_custom: bool,
    /// None is single selection; Some enables multiple selection.
    pub selection: Option<BTreeSet<ListItemId>>,
}
impl ComboBoxView {
    pub fn themed(
        theme: &Theme,
        items: Vec<ListItem>,
        selected: Option<ListItemId>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            list: ListBoxView::themed(theme, items, families.clone(), selected, action),
            button: ButtonView::themed(theme, "", families.clone(), action),
            editor: TextInputView::themed(theme, "", families, action),
            placeholder: "请选择".into(),
            query: String::new(),
            searchable: true,
            allow_custom: false,
            selection: None,
        }
    }
    pub fn build(&self) -> View {
        let mut button = self.button.clone();
        button.disabled |= self.list.disabled;
        button.label = if let Some(ids) = &self.selection {
            self.list
                .items
                .iter()
                .filter(|i| ids.contains(&i.id))
                .map(|i| i.label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            self.list
                .selected
                .and_then(|id| self.list.items.iter().find(|i| i.id == id))
                .map(|i| i.label.clone())
                .unwrap_or_default()
        };
        if button.label.is_empty() {
            button.label = self.placeholder.clone();
        }
        button.trailing_icon = Some(crate::Icon::builtin(crate::IconKind::ChevronDown));
        View::new(ActionButtonView {
            button,
            payload: ComboBoxEvent::Open,
        })
    }
    /// Case-insensitive substring matching; applications may pre-filter items for remote search.
    pub fn filtered_items(&self) -> Vec<ListItem> {
        let query = self.query.to_lowercase();
        self.list
            .items
            .iter()
            .filter(|i| !self.searchable || i.label.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }
    pub fn popup(&self, anchor: Rect, height: f64, dismiss: ActionId) -> View {
        let mut list = self.list.clone();
        list.items = self.filtered_items();
        let visible: BTreeSet<_> = list.items.iter().map(|i| i.id).collect();
        list.selected = list.selected.filter(|id| visible.contains(id));
        list.multi_selection = self
            .selection
            .as_ref()
            .map(|ids| ids.intersection(&visible).copied().collect());
        list.activation_only = self.selection.is_none();
        let mut children = Vec::new();
        if self.searchable || self.allow_custom {
            let mut editor = self.editor.clone();
            editor.text = self.query.clone();
            editor.action = self.list.action;
            editor.disabled |= self.list.disabled;
            editor.placeholder = if self.allow_custom {
                "输入或搜索"
            } else {
                "搜索"
            }
            .into();
            children.push(
                View::new(QueryView {
                    editor,
                    allow_custom: self.allow_custom,
                })
                .keyed("query")
                .styled(LayoutStyle {
                    padding: Edges::all(if self.button.style.metrics.min_height > 0 {
                        6.0
                    } else {
                        0.0
                    }),
                    shrink: 0.0,
                    ..Default::default()
                }),
            );
        }
        children.push(
            View::new(ResultsView {
                list,
                selection: self.selection.clone(),
                visible,
            })
            .keyed("results")
            .styled(LayoutStyle {
                grow: 1.0,
                min_height: Dimension::Length(0.0),
                ..Default::default()
            }),
        );
        PopupView::new(
            PopupPlacement::Anchor(anchor),
            Size::new(anchor.size.width.max(200.0), height),
            dismiss,
        )
        .build(
            column(children)
                .background(self.list.style.normal.background)
                .border(self.editor.style.border)
                .radius(self.button.style.metrics.panel_radius)
                .style(|s| {
                    if self.button.style.metrics.min_height > 0 {
                        s.padding = Edges::all(6.0);
                        s.gap = Size::new(0.0, 4.0);
                    }
                    s.width = Dimension::Percent(1.0);
                    s.height = Dimension::Percent(1.0);
                })
                .build(),
        )
    }
}
#[derive(Clone)]
struct QueryView {
    editor: TextInputView,
    allow_custom: bool,
}
struct Query {
    editor: TextInput,
    allow_custom: bool,
}
impl ViewSpec for QueryView {
    type Widget = Query;
    fn build(&self) -> Query {
        Query {
            editor: self.editor.build(),
            allow_custom: self.allow_custom,
        }
    }
    fn update(&self, w: &mut Query) -> Invalidation {
        w.allow_custom = self.allow_custom;
        self.editor.update(&mut w.editor)
    }
}
impl Query {
    fn handle(
        &mut self,
        c: EventContext,
        e: &WidgetEvent,
        s: &mut EventServices<'_>,
    ) -> EventResponse {
        let mut r = self.editor.event_with_services(c, e, s);
        r.actions = r
            .actions
            .into_iter()
            .filter_map(|a| match a.payload::<TextInputEvent>() {
                Some(TextInputEvent::Changed(text)) => Some(WidgetAction::new(
                    a.id,
                    ComboBoxEvent::QueryChanged(text.clone()),
                )),
                Some(TextInputEvent::Submitted(text)) if self.allow_custom => Some(
                    WidgetAction::new(a.id, ComboBoxEvent::Submitted(text.clone())),
                ),
                Some(TextInputEvent::Submitted(_)) => None,
                _ => Some(a),
            })
            .collect();
        r
    }
}
impl Widget for Query {
    fn enabled(&self) -> bool {
        self.editor.enabled()
    }
    fn focusable(&self) -> bool {
        self.editor.focusable()
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.editor.measure(c)
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.editor.prepare(c)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.editor.paint(c, p);
    }
    fn animate(&mut self, t: Duration) -> AnimationUpdate {
        self.editor.animate(t)
    }
    fn text_input(&self, c: PaintContext) -> Option<TextInputState> {
        self.editor.text_input(c)
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        self.handle(
            c,
            e,
            &mut EventServices {
                clipboard: &mut zui_api::clipboard::UnavailableClipboard,
            },
        )
    }
    fn event_with_services(
        &mut self,
        c: EventContext,
        e: &WidgetEvent,
        s: &mut EventServices<'_>,
    ) -> EventResponse {
        self.handle(c, e, s)
    }
}
#[derive(Clone)]
struct ResultsView {
    list: ListBoxView,
    selection: Option<BTreeSet<ListItemId>>,
    visible: BTreeSet<ListItemId>,
}
struct Results {
    list: ListBox,
    properties: ResultsView,
}
impl ViewSpec for ResultsView {
    type Widget = Results;
    fn build(&self) -> Results {
        Results {
            list: self.list.build(),
            properties: self.clone(),
        }
    }
    fn update(&self, w: &mut Results) -> Invalidation {
        let r = self.list.update(&mut w.list);
        w.properties = self.clone();
        r
    }
}
impl Widget for Results {
    fn enabled(&self) -> bool {
        self.list.enabled()
    }
    fn focusable(&self) -> bool {
        self.list.focusable()
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.list.measure(c)
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.list.prepare(c)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.list.paint(c, p);
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        let mut r = self.list.event(c, e);
        for a in &mut r.actions {
            if let Some(change) = a.payload::<ListMultiSelectionChanged>() {
                let mut selected: BTreeSet<_> = self
                    .properties
                    .selection
                    .iter()
                    .flatten()
                    .filter(|id| !self.properties.visible.contains(id))
                    .copied()
                    .collect();
                selected.extend(change.selected.iter().copied());
                *a = WidgetAction::new(a.id, ComboBoxEvent::SelectionChanged(selected));
            } else if let Some(change) = a.payload::<ListSelectionChanged>() {
                *a = WidgetAction::new(a.id, ComboBoxEvent::Selected(change.selected));
            }
        }
        r
    }
}
