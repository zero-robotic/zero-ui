//! Tabs compose a single-choice header and one keyed, lazily built page.
use crate::{
    Container, SegmentChanged, SegmentId, SegmentOption, SegmentedControl, SegmentedControlView,
    SegmentedStyle, column,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{LayoutError, LayoutKind, LayoutStyle},
    text::TextStyle,
    view::{Key, View, ViewSpec},
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::{Point, Rect, Size};

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
pub struct TabCloseRequested {
    pub id: TabId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabReorderRequested {
    pub id: TabId,
    pub to_index: usize,
}
/// The application creates/moves windows. Position is relative to the tab header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabDetachRequested {
    pub id: TabId,
    pub position: Point,
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
    /// Content-width labels grouped at the start; false restores equal-width tabs.
    pub compact: bool,
    pub closable: BTreeSet<TabId>,
    pub reorderable: bool,
    pub detachable: bool,
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
            compact: true,
            closable: BTreeSet::new(),
            reorderable: false,
            detachable: false,
            selected,
            disabled: false,
            action,
            text_style: header.text_style,
            header_style: SegmentedStyle {
                gap: 0.0,
                ..header.style
            },
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
        let mut header_style = self.header_style;
        header_style.gap = 0.0;
        if !self.closable.is_empty() {
            header_style.padding_x = header_style.padding_x.max(22.0);
        }
        let header = TabBarView {
            compact: self.compact,
            closable: self.closable,
            reorderable: self.reorderable,
            detachable: self.detachable,
            segments: SegmentedControlView {
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
                style: header_style,
            },
        };
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
#[derive(Clone)]
struct TabBarView {
    compact: bool,
    segments: SegmentedControlView,
    closable: BTreeSet<TabId>,
    reorderable: bool,
    detachable: bool,
}
struct TabBar {
    segments: SegmentedControl,
    properties: TabBarView,
    press: Option<(TabId, Point, bool, bool)>,
}
impl TabBar {
    fn hit(&self, point: Point, size: Size) -> Option<(TabId, usize, bool)> {
        if point.x < 0.0 || point.y < 0.0 || point.x >= size.width || point.y >= size.height {
            return None;
        }
        let (index, slot) = self
            .segments
            .slots()
            .iter()
            .enumerate()
            .find(|(_, slot)| slot.contains(point))?;
        let option = self
            .properties
            .segments
            .options
            .get(index)
            .filter(|o| !o.disabled)?;
        let id = TabId(option.id.0);
        Some((
            id,
            index,
            self.properties.closable.contains(&id)
                && slot.size.width >= 20.0
                && point.x >= slot.origin.x + slot.size.width - 20.0,
        ))
    }

    fn action<T: 'static>(&self, event: T) -> EventResponse {
        EventResponse {
            handled: true,
            actions: alloc::vec![WidgetAction::new(self.properties.segments.action, event)],
            ..Default::default()
        }
    }
}
impl ViewSpec for TabBarView {
    type Widget = TabBar;
    fn build(&self) -> TabBar {
        let mut segments = self.segments.build();
        segments.compact = self.compact;
        segments.joined = true;
        TabBar {
            segments,
            properties: self.clone(),
            press: None,
        }
    }
    fn update(&self, widget: &mut TabBar) -> Invalidation {
        if self.segments.options != widget.properties.segments.options
            || self.segments.disabled
            || self.segments.action != widget.properties.segments.action
        {
            widget.press = None;
        }
        let mut result = self.segments.update(&mut widget.segments);
        if self.compact != widget.properties.compact {
            widget.segments.compact = self.compact;
            widget.press = None;
            result = Invalidation::Layout;
        }
        let changed = self.closable != widget.properties.closable;
        widget.properties = self.clone();
        if changed {
            result.max(Invalidation::Paint)
        } else {
            result
        }
    }
}
impl Widget for TabBar {
    fn enabled(&self) -> bool {
        self.segments.enabled()
    }
    fn focusable(&self) -> bool {
        self.segments.focusable()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.segments.measure(context)
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.segments.prepare(context)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        self.segments.paint(context, painter);
        let v = &self.properties.segments;
        for (index, tab) in v.options.iter().enumerate() {
            let Some(slot) = self.segments.slots().get(index) else {
                continue;
            };
            if self.properties.closable.contains(&TabId(tab.id.0)) && slot.size.width >= 20.0 {
                crate::Icon::builtin(crate::IconKind::Close).paint(
                    Rect::new(
                        context.content_bounds.origin.x + slot.origin.x + slot.size.width - 18.0,
                        context.content_bounds.origin.y + (slot.size.height - 14.0) / 2.0,
                        14.0,
                        14.0,
                    ),
                    if v.disabled || tab.disabled {
                        v.style.disabled.text
                    } else {
                        v.style.normal.text
                    },
                    painter,
                );
            }
        }
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        if matches!(
            event,
            WidgetEvent::PointerCaptureLost | WidgetEvent::FocusChanged(false)
        ) {
            self.press = None;
        }
        if self.enabled() && context.phase == EventPhase::Target {
            if let WidgetEvent::Input(InputEvent::Key(k)) = event
                && k.pressed
                && !k.repeat
            {
                if k.key == zui_api::event::Key::Escape && self.press.take().is_some() {
                    let mut r = self
                        .segments
                        .event(context, &WidgetEvent::PointerCaptureLost);
                    r.capture = CaptureRequest::Release;
                    r.handled = true;
                    return r;
                }
                if (k.modifiers.control || k.modifiers.super_key)
                    && !k.modifiers.alt
                    && let Some(id) = self.properties.segments.selected.map(|id| TabId(id.0))
                {
                    if k.key == zui_api::event::Key::Character("w".into())
                        && self.properties.closable.contains(&id)
                    {
                        return self.action(TabCloseRequested { id });
                    }
                    if self.properties.reorderable
                        && k.modifiers.shift
                        && matches!(
                            k.key,
                            zui_api::event::Key::ArrowLeft | zui_api::event::Key::ArrowRight
                        )
                    {
                        let options = &self.properties.segments.options;
                        if let Some(index) = options.iter().position(|i| i.id.0 == id.0) {
                            let to_index = if k.key == zui_api::event::Key::ArrowLeft {
                                index.saturating_sub(1)
                            } else {
                                (index + 1).min(options.len() - 1)
                            };
                            if to_index != index {
                                return self.action(TabReorderRequested { id, to_index });
                            }
                        }
                    }
                }
            }
            if let (Some(point), Some(geometry)) = (context.local_position, context.geometry) {
                let point = Point::new(
                    point.x - geometry.content_bounds.origin.x,
                    point.y - geometry.content_bounds.origin.y,
                );
                match event {
                    WidgetEvent::Input(InputEvent::PointerButton {
                        button: PointerButton::Primary,
                        pressed: true,
                        ..
                    }) => {
                        if let Some((id, _, close)) = self.hit(point, geometry.content_bounds.size)
                        {
                            self.press = Some((id, point, close, false));
                        }
                    }
                    WidgetEvent::Input(InputEvent::PointerMoved(_)) => {
                        if let Some((_, start, close, moved)) = &mut self.press
                            && !*close
                        {
                            *moved |= (point.x - start.x).abs() + (point.y - start.y).abs() >= 6.0;
                        }
                    }
                    WidgetEvent::Input(InputEvent::PointerButton {
                        button: PointerButton::Primary,
                        pressed: false,
                        ..
                    }) => {
                        if let Some((id, _, close, moved)) = self.press.take() {
                            let hit = self.hit(point, geometry.content_bounds.size);
                            let action = if close && hit.is_some_and(|h| h.0 == id && h.2) {
                                Some(self.action(TabCloseRequested { id }))
                            } else if moved
                                && self.properties.detachable
                                && (point.y < -16.0
                                    || point.y > geometry.content_bounds.size.height + 16.0)
                            {
                                Some(self.action(TabDetachRequested {
                                    id,
                                    position: point,
                                }))
                            } else if moved && self.properties.reorderable {
                                hit.filter(|h| h.0 != id)
                                    .map(|h| self.action(TabReorderRequested { id, to_index: h.1 }))
                            } else {
                                None
                            };
                            if let Some(mut r) = action {
                                self.segments
                                    .event(context, &WidgetEvent::PointerCaptureLost);
                                r.capture = CaptureRequest::Release;
                                r.invalidation = Invalidation::Paint;
                                return r;
                            }
                            if close {
                                let mut r = self
                                    .segments
                                    .event(context, &WidgetEvent::PointerCaptureLost);
                                r.capture = CaptureRequest::Release;
                                r.handled = true;
                                return r;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut response = self.segments.event(context, event);
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
