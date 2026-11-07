//! Tabs compose a single-choice header and one keyed, lazily built page.
use crate::{
    Container, SegmentChanged, SegmentId, SegmentOption, SegmentedControl, SegmentedControlView,
    SegmentedStyle, column,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::{EventContext, EventResponse, WidgetEvent},
    layout::{LayoutError, LayoutKind, LayoutStyle},
    text::TextStyle,
    view::{Key, View, ViewSpec},
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::Size;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TabId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabOption {
    pub id: TabId,
    pub label: String,
    pub disabled: bool,
}
impl TabOption {
    pub fn new(id: TabId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
        }
    }
}
/// A request; the application confirms it by rebuilding with `selected`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabChanged {
    pub selected: TabId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabsError {
    DuplicateId(TabId),
    UnknownSelection(TabId),
}
impl core::fmt::Display for TabsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "duplicate tab ID {}", id.0),
            Self::UnknownSelection(id) => write!(f, "unknown selected tab ID {}", id.0),
        }
    }
}
impl core::error::Error for TabsError {}

/// Inactive pages are unmounted. Keep persistent page data in the application.
/// The factory runs once for the selected page, or never for `None`/invalid IDs.
pub struct TabsView {
    pub tabs: Vec<TabOption>,
    pub selected: Option<TabId>,
    /// Disables header interaction, not the selected page's controls.
    pub disabled: bool,
    pub action: ActionId,
    pub text_style: TextStyle,
    pub header_style: SegmentedStyle,
    pub layout: LayoutStyle,
    pub header_layout: LayoutStyle,
    pub panel_layout: LayoutStyle,
    pub key: Option<Key>,
}
impl TabsView {
    pub fn themed(
        theme: &Theme,
        tabs: Vec<TabOption>,
        families: Vec<String>,
        selected: Option<TabId>,
        action: ActionId,
    ) -> Self {
        let header = SegmentedControlView::themed(theme, Vec::new(), families, None, action);
        Self {
            tabs,
            selected,
            disabled: false,
            action,
            text_style: header.text_style,
            header_style: header.style,
            layout: LayoutStyle::default(),
            header_layout: LayoutStyle {
                shrink: 0.0,
                ..Default::default()
            },
            panel_layout: LayoutStyle::default(),
            key: None,
        }
    }
    pub fn build(self, content: impl FnOnce(TabId) -> View) -> Result<View, TabsError> {
        let mut ids = BTreeSet::new();
        for tab in &self.tabs {
            if !ids.insert(tab.id) {
                return Err(TabsError::DuplicateId(tab.id));
            }
        }
        if let Some(id) = self.selected
            && !ids.contains(&id)
        {
            return Err(TabsError::UnknownSelection(id));
        }
        let header = TabBarView(SegmentedControlView {
            options: self
                .tabs
                .into_iter()
                .map(|tab| SegmentOption {
                    id: SegmentId(tab.id.0),
                    label: tab.label,
                    disabled: tab.disabled,
                })
                .collect(),
            selected: self.selected.map(|id| SegmentId(id.0)),
            disabled: self.disabled,
            action: self.action,
            text_style: self.text_style,
            style: self.header_style,
        });
        let mut header_layout = self.header_layout;
        header_layout.kind = LayoutKind::Leaf;
        let mut children = Vec::with_capacity(2);
        children.push(View::new(header).keyed("header").styled(header_layout));
        if let Some(id) = self.selected {
            children.push(
                Container::new(content(id))
                    .keyed(Key::Number(id.0))
                    .style(|style| *style = self.panel_layout)
                    .build(),
            );
        }
        let mut root = column(children).style(|style| *style = self.layout).build();
        root.key = self.key;
        Ok(root)
    }
}

// Adapter stays in the controls layer: the runtime transports opaque actions.
struct TabBarView(SegmentedControlView);
struct TabBar(SegmentedControl);
impl ViewSpec for TabBarView {
    type Widget = TabBar;
    fn build(&self) -> TabBar {
        TabBar(self.0.build())
    }
    fn update(&self, widget: &mut TabBar) -> Invalidation {
        self.0.update(&mut widget.0)
    }
}
impl Widget for TabBar {
    fn enabled(&self) -> bool {
        self.0.enabled()
    }
    fn focusable(&self) -> bool {
        self.0.focusable()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.0.measure(context)
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.0.prepare(context)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        self.0.paint(context, painter);
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        let mut response = self.0.event(context, event);
        for action in &mut response.actions {
            if let Some(changed) = action.payload::<SegmentChanged>() {
                *action = WidgetAction::new(
                    action.id,
                    TabChanged {
                        selected: TabId(changed.selected.0),
                    },
                );
            }
        }
        response
    }
}
