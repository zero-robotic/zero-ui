//! Controlled collapsible content, composed entirely in the controls layer.
use crate::{Button, ButtonActivated, ButtonStyle, ButtonView, Container, column};
use alloc::{format, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::{
        CaptureRequest, EventContext, EventPhase, EventResponse, FocusRequest, InputEvent,
        Key as InputKey, WidgetEvent,
    },
    layout::{Edges, LayoutError, LayoutKind, LayoutStyle},
    text::{TextAlign, TextStyle},
    view::{Key, View, ViewSpec},
    widget::{Invalidation, Lifecycle, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::{NodeId, Size};

/// Requests a new expanded value; the application owns the confirmed state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisclosureChanged {
    pub expanded: bool,
}

// The composition owns request semantics; the header only triggers activation.
pub(crate) trait HeaderRequest: Clone + PartialEq + 'static {
    fn activate(&self, action: ActionId) -> WidgetAction;
    fn lifecycle(&self, _id: NodeId, _event: Lifecycle) {}
    fn navigate(&self, _key: &InputKey) -> Option<NodeId> {
        None
    }
}
impl HeaderRequest for DisclosureChanged {
    fn activate(&self, action: ActionId) -> WidgetAction {
        WidgetAction::new(action, *self)
    }
}

/// A header and optional content subtree. Collapsed content is unmounted.
pub struct DisclosureView {
    pub label: String,
    pub expanded: bool,
    /// Disables only the header, without disabling controls in expanded content.
    pub disabled: bool,
    pub action: ActionId,
    pub text_style: TextStyle,
    pub header_style: ButtonStyle,
    pub layout: LayoutStyle,
    pub header_layout: LayoutStyle,
    pub panel_layout: LayoutStyle,
    pub key: Option<Key>,
}
impl DisclosureView {
    pub fn themed(
        theme: &Theme,
        label: impl Into<String>,
        families: Vec<String>,
        expanded: bool,
        action: ActionId,
    ) -> Self {
        let mut header = ButtonView::themed(theme, label, families, action);
        header.text_style.align = TextAlign::Start;
        Self {
            label: header.label,
            expanded,
            disabled: false,
            action,
            text_style: header.text_style,
            header_style: header.style,
            layout: LayoutStyle::default(),
            header_layout: LayoutStyle {
                padding: Edges::all(12.0),
                shrink: 0.0,
                ..Default::default()
            },
            panel_layout: LayoutStyle::default(),
            key: None,
        }
    }
    /// Calls the factory exactly once when expanded, never when collapsed.
    pub fn build(self, content: impl FnOnce() -> View) -> View {
        let request = DisclosureChanged {
            expanded: !self.expanded,
        };
        self.build_with_request(content, request)
    }
    // Higher-level compositions supply an opaque payload; Disclosure knows no group types.
    pub(crate) fn build_with_request<A: HeaderRequest>(
        self,
        content: impl FnOnce() -> View,
        request: A,
    ) -> View {
        let header = HeaderView {
            button: ButtonView {
                trailing_icon: None,
                label: format!("{}  {}", if self.expanded { "−" } else { "+" }, self.label),
                text_style: self.text_style,
                style: self.header_style,
                action: self.action,
                disabled: self.disabled,
            },
            request,
        };
        let mut layout = self.header_layout;
        layout.kind = LayoutKind::Leaf;
        let mut children = Vec::with_capacity(2);
        children.push(View::new(header).keyed("header").styled(layout));
        if self.expanded {
            children.push(
                Container::new(content())
                    .keyed("content")
                    .style(|s| *s = self.panel_layout)
                    .build(),
            );
        }
        let mut root = column(children).style(|s| *s = self.layout).build();
        root.key = self.key;
        root
    }
}
struct HeaderView<A> {
    button: ButtonView,
    request: A,
}
struct Header<A> {
    id: Option<NodeId>,
    button: Button,
    request: A,
}
impl<A: HeaderRequest> ViewSpec for HeaderView<A> {
    type Widget = Header<A>;
    fn build(&self) -> Header<A> {
        Header {
            id: None,
            button: self.button.build(),
            request: self.request.clone(),
        }
    }
    fn update(&self, widget: &mut Header<A>) -> Invalidation {
        let changed = widget.request != self.request;
        if changed {
            widget.button.cancel_press();
        }
        if let Some(id) = widget.id {
            widget.request.lifecycle(id, Lifecycle::Unmounted);
            self.request.lifecycle(id, Lifecycle::Mounted);
        }
        widget.request = self.request.clone();
        let invalidation = self.button.update(&mut widget.button);
        if changed {
            invalidation.max(Invalidation::Paint)
        } else {
            invalidation
        }
    }
}
impl<A: HeaderRequest> Widget for Header<A> {
    fn lifecycle(&mut self, id: NodeId, event: Lifecycle) {
        self.id = (event == Lifecycle::Mounted).then_some(id);
        self.request.lifecycle(id, event);
    }
    fn enabled(&self) -> bool {
        self.button.enabled()
    }
    fn focusable(&self) -> bool {
        self.button.focusable()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.button.measure(context)
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.button.prepare(context)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        self.button.paint(context, painter);
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        if context.phase == EventPhase::Target
            && self.enabled()
            && let WidgetEvent::Input(InputEvent::Key(key)) = event
            && key.pressed
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.super_key
            && let Some(target) = self.request.navigate(&key.key)
        {
            self.button.cancel_press();
            return EventResponse {
                handled: true,
                focus: FocusRequest::Target(target),
                capture: CaptureRequest::Release,
                invalidation: Invalidation::Paint,
                ..Default::default()
            };
        }
        let mut response = self.button.event(context, event);
        for action in &mut response.actions {
            if action.payload::<ButtonActivated>().is_some() {
                *action = self.request.activate(action.id);
            }
        }
        response
    }
}
