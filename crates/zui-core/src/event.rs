use crate::{Tree, TreeError, geometry::content_bounds, presentation::Presentation};
use alloc::vec::Vec;
use zui_api::{
    action::ActionEvent,
    event::{
        CaptureRequest, EventContext, EventGeometry, EventPhase, FocusRequest, InputEvent, Key,
        PointerButton, ScrollDelta, WidgetEvent,
    },
    widget::{Invalidation, PaintContext},
};
use zui_types::{NodeId, Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputTarget {
    pub id: NodeId,
    pub generation: u64,
    /// Window-relative logical coordinates, intersected with ancestor clips.
    pub cursor_area: Rect,
}

#[derive(Default)]
pub(crate) struct EventState {
    pub(crate) reveal_focus: Option<NodeId>,
    focus: Option<NodeId>,
    capture: Option<NodeId>,
    hover: Option<NodeId>,
    position: Option<Point>,
    buttons: Vec<PointerButton>,
    modifiers: zui_api::event::Modifiers,
}
#[derive(Debug)]
pub struct DispatchResult {
    pub actions: Vec<ActionEvent>,
    pub target: Option<NodeId>,
    pub handled: bool,
    /// Tree's accumulated invalidation after dispatch, including direct notifications.
    pub invalidation: Invalidation,
}
#[derive(Default)]
struct Requests {
    focus: Option<Option<NodeId>>,
    capture: Option<Option<NodeId>>,
}
impl Tree {
    pub fn text_input(&self) -> Result<Option<TextInputTarget>, TreeError> {
        if self.dirty == Invalidation::Layout {
            return Err(TreeError::LayoutRequired);
        }
        let Some(id) = self.events.focus else {
            return Ok(None);
        };
        let node = &self.nodes[&id];
        if !node.widget.enabled() || !node.widget.focusable() {
            return Ok(None);
        }
        let Some(state) = node.widget.text_input(PaintContext {
            id,
            size: node.layout.bounds.size,
            content_bounds: content_bounds(node, node.layout.bounds),
        }) else {
            return Ok(None);
        };
        if !state.cursor_area.is_valid() {
            return Err(TreeError::InvalidTextInputArea(id));
        }
        let presentation = self.presentation()?;
        let path = self.event_path(id, &presentation);
        let origin = path.last().expect("focused path").1;
        let mut area = Rect::new(
            origin.x + state.cursor_area.origin.x,
            origin.y + state.cursor_area.origin.y,
            state.cursor_area.size.width,
            state.cursor_area.size.height,
        );
        if !area.is_valid() {
            return Err(TreeError::InvalidTextInputArea(id));
        }
        for (ancestor, origin) in path {
            let node = &self.nodes[&ancestor];
            let border_clip = node.clip.then_some(Rect::new(
                0.0,
                0.0,
                node.layout.bounds.size.width,
                node.layout.bounds.size.height,
            ));
            // A node's child viewport clips descendants, never its own IME area.
            let child_clip = (ancestor != id)
                .then_some(presentation[&ancestor].children.clip)
                .flatten();
            for clip in border_clip.into_iter().chain(child_clip) {
                if clip.size.width <= 0.0 || clip.size.height <= 0.0 {
                    return Ok(None);
                }
                let left = area.origin.x.max(origin.x + clip.origin.x);
                let top = area.origin.y.max(origin.y + clip.origin.y);
                let right = (area.origin.x + area.size.width)
                    .min(origin.x + clip.origin.x + clip.size.width);
                let bottom = (area.origin.y + area.size.height)
                    .min(origin.y + clip.origin.y + clip.size.height);
                if right < left || bottom <= top {
                    return Ok(None);
                }
                area = Rect::new(left, top, right - left, bottom - top);
            }
        }
        if area.size.height <= 0.0 {
            return Ok(None);
        }
        Ok(Some(TextInputTarget {
            id,
            generation: state.generation,
            cursor_area: area,
        }))
    }
    pub fn focused(&self) -> Option<NodeId> {
        self.events.focus
    }
    /// Focus an existing enabled/focusable node, or clear focus with None.
    /// Validation errors occur before notifications. Geometry is not required:
    /// reveal is deferred if layout is dirty, as for event-driven focus changes.
    /// Returns whether ownership changed; repeating a successful request does
    /// not re-reveal a target after intentional manual scrolling.
    /// A reveal callback error can occur after focus notifications and is not
    /// rolled back. This does not request native-window focus or pointer capture.
    pub fn request_focus(&mut self, target: Option<NodeId>) -> Result<bool, TreeError> {
        if let Some(id) = target {
            let node = self.get(id)?;
            if !node.widget.enabled() || !node.widget.focusable() || !self.in_focus_scope(id) {
                return Err(TreeError::NotFocusable(id));
            }
        }
        let changed = self.events.focus != target;
        self.set_focus(target);
        self.reveal_pending_focus()?;
        Ok(changed)
    }
    fn can_focus(&self, id: NodeId) -> bool {
        self.in_focus_scope(id)
            && self
                .nodes
                .get(&id)
                .is_some_and(|node| node.widget.enabled() && node.widget.focusable())
    }
    pub fn pointer_capture(&self) -> Option<NodeId> {
        self.events.capture
    }
    pub fn hovered(&self) -> Option<NodeId> {
        self.events.hover
    }

    /// Dispatch against committed geometry. Call layout first after any geometry
    /// invalidation. Touch routing is reserved for a separate multi-contact path.
    pub fn dispatch_input(&mut self, input: InputEvent) -> Result<DispatchResult, TreeError> {
        self.dispatch_input_with_services(
            input,
            &mut zui_api::event::EventServices {
                clipboard: &mut zui_api::clipboard::UnavailableClipboard,
            },
        )
    }
    pub fn dispatch_input_with_services(
        &mut self,
        input: InputEvent,
        services: &mut zui_api::event::EventServices<'_>,
    ) -> Result<DispatchResult, TreeError> {
        if matches!(input, InputEvent::Touch { .. }) {
            return Err(TreeError::UnsupportedInput);
        }
        if self.dirty == Invalidation::Layout {
            return Err(TreeError::LayoutRequired);
        }
        match &input {
            InputEvent::ModifiersChanged(m) => self.events.modifiers = *m,
            InputEvent::Key(k) => self.events.modifiers = k.modifiers,
            _ => {}
        }
        let position = match &input {
            InputEvent::PointerMoved(point) => Some(*point),
            InputEvent::PointerButton { position, .. } => *position,
            _ => None,
        };
        if position.is_some_and(|p| !p.is_finite()) {
            return Err(TreeError::InvalidInput);
        }
        if let InputEvent::Scroll(ScrollDelta::Lines { x, y } | ScrollDelta::Pixels { x, y }) =
            &input
            && (!x.is_finite() || !y.is_finite())
        {
            return Err(TreeError::InvalidInput);
        }
        let presentation = self.presentation()?;
        self.prune_input();
        if let Some(point) = position {
            self.events.position = Some(point);
        }
        let mouse = matches!(
            input,
            InputEvent::PointerMoved(_)
                | InputEvent::PointerButton { .. }
                | InputEvent::PointerEntered
                | InputEvent::PointerLeft
                | InputEvent::Scroll(_)
        );
        if mouse
            && !matches!(input, InputEvent::PointerLeft)
            && let Some(point) = self.events.position
        {
            // Re-hit-test after geometry changes even when this input (e.g. a
            // wheel event) supplies no new position.
            let hit = self
                .hit_node(self.root, point, &presentation)
                .map(|hit| hit.id);
            self.set_hover(hit);
        }
        if let InputEvent::PointerButton {
            button, pressed, ..
        } = &input
        {
            if *pressed {
                if !self.events.buttons.contains(button) {
                    self.events.buttons.push(*button);
                }
            } else {
                self.events.buttons.retain(|b| b != button);
            }
        }
        if matches!(input, InputEvent::PointerLeft) {
            self.events.position = None;
            self.set_hover(None);
        }
        let target = if mouse {
            self.events.capture.or(self.events.hover)
        } else {
            self.events.focus
        };
        let mut requests = Requests::default();
        let mut handled = false;
        let mut actions = Vec::new();
        if let Some(target) = target.filter(|id| self.in_focus_scope(*id)) {
            let mut path = self.event_path(target, &presentation);
            if let Some(&(scope, _)) = self.focus_scopes.last()
                && let Some(start) = path.iter().position(|(id, _)| *id == scope)
            {
                path.drain(..start);
            }
            let last = path.len() - 1;
            let mut route: Vec<_> = path[..last]
                .iter()
                .map(|&(id, origin)| (id, origin, EventPhase::Capture))
                .collect();
            route.push((target, path[last].1, EventPhase::Target));
            route.extend(
                path[..last]
                    .iter()
                    .rev()
                    .map(|&(id, origin)| (id, origin, EventPhase::Bubble)),
            );
            for (id, origin, phase) in route {
                let local = if mouse {
                    self.events
                        .position
                        .map(|p| Point::new(p.x - origin.x, p.y - origin.y))
                } else {
                    None
                };
                let mut localized = input.clone();
                match &mut localized {
                    InputEvent::PointerMoved(point) => {
                        if let Some(local) = local {
                            *point = local;
                        }
                    }
                    InputEvent::PointerButton { position, .. } => *position = local,
                    _ => {}
                }
                let node = self.nodes.get_mut(&id).expect("snapshot event path");
                let geometry = EventGeometry {
                    size: node.layout.bounds.size,
                    content_bounds: content_bounds(node, node.layout.bounds),
                };
                let response = node.widget.event_with_services(
                    EventContext {
                        modifiers: self.events.modifiers,
                        id,
                        target,
                        phase,
                        local_position: local,
                        geometry: Some(geometry),
                    },
                    &WidgetEvent::Input(localized),
                    services,
                );
                self.dirty = self.dirty.max(response.invalidation);
                if response.invalidation != Invalidation::None {
                    self.paint_dirty.insert(id);
                }
                handled |= response.handled;
                actions.extend(
                    response
                        .actions
                        .into_iter()
                        .map(|action| ActionEvent { source: id, action }),
                );
                if requests.capture.is_none() {
                    match response.capture {
                        CaptureRequest::Capture
                            if mouse
                                && node.widget.enabled()
                                && !self.events.buttons.is_empty() =>
                        {
                            requests.capture = Some(Some(id))
                        }
                        CaptureRequest::Release if self.events.capture == Some(id) => {
                            requests.capture = Some(None)
                        }
                        _ => {}
                    }
                }
                let requester_enabled = node.widget.enabled();
                if requests.focus.is_none() {
                    let candidate = match response.focus {
                        FocusRequest::Focus => Some(id),
                        FocusRequest::Target(target) if requester_enabled => Some(target),
                        _ => None,
                    };
                    if let Some(candidate) = candidate.filter(|&id| self.can_focus(id)) {
                        requests.focus = Some(Some(candidate));
                    } else if response.focus == FocusRequest::Clear {
                        requests.focus = Some(None);
                    }
                }
                if response.stop_propagation {
                    break;
                }
            }
        }
        // A later callback may change the requested node's eligibility. Invalid
        // requests leave ownership unchanged; dispatch itself still succeeds.
        if let Some(focus) = requests.focus
            && focus.is_none_or(|id| self.can_focus(id))
        {
            self.set_focus(focus);
        }
        if let Some(capture) = requests.capture {
            self.set_capture(capture);
        }
        if matches!(input, InputEvent::PointerButton { pressed: false, .. })
            && self.events.buttons.is_empty()
        {
            self.set_capture(None);
        }
        if !handled
            && let InputEvent::Key(key) = &input
            && key.key == Key::Tab
            && key.pressed
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.super_key
        {
            handled = self.navigate_focus(key.modifiers.shift, &mut actions);
        }
        self.reveal_pending_focus()?;
        // Active scopes consume otherwise-unhandled input before application shortcuts.
        handled |= !self.focus_scopes.is_empty();
        Ok(DispatchResult {
            actions,
            target,
            handled,
            invalidation: self.dirty,
        })
    }
    /// Window focus loss/suspension cancels input ownership and notifies controls.
    pub fn cancel_input(&mut self) {
        self.set_capture(None);
        self.set_focus(None);
        self.set_hover(None);
        self.events.buttons.clear();
        self.events.modifiers = Default::default();
        self.events.position = None;
    }
    fn event_path(&self, mut id: NodeId, presentation: &Presentation) -> Vec<(NodeId, Point)> {
        let mut ids = Vec::new();
        loop {
            ids.push(id);
            let Some(parent) = self.nodes[&id].parent else {
                break;
            };
            id = parent;
        }
        ids.reverse();
        ids.into_iter()
            .map(|id| (id, presentation[&id].origin))
            .collect()
    }

    fn direct(&mut self, id: NodeId, event: WidgetEvent) {
        if let Some(node) = self.nodes.get_mut(&id) {
            let response = node.widget.event(
                EventContext {
                    modifiers: self.events.modifiers,
                    id,
                    target: id,
                    phase: EventPhase::Direct,
                    local_position: None,
                    geometry: None,
                },
                &event,
            );
            // Ownership requests from notifications are ignored to avoid recursive transfers.
            self.dirty = self.dirty.max(response.invalidation);
            if response.invalidation != Invalidation::None {
                self.paint_dirty.insert(id);
            }
        }
    }
    pub(crate) fn set_focus(&mut self, focus: Option<NodeId>) {
        if self.events.focus == focus {
            return;
        }
        if let Some(old) = self.events.focus.take() {
            self.direct(old, WidgetEvent::FocusChanged(false));
        }
        self.events.focus = focus;
        self.events.reveal_focus = focus;
        if let Some(id) = focus {
            self.direct(id, WidgetEvent::FocusChanged(true));
        }
    }
    pub(crate) fn set_capture(&mut self, capture: Option<NodeId>) {
        if self.events.capture == capture {
            return;
        }
        if let Some(old) = self.events.capture.take() {
            self.direct(old, WidgetEvent::PointerCaptureLost);
        }
        self.events.capture = capture;
    }
    fn set_hover(&mut self, hover: Option<NodeId>) {
        if self.events.hover == hover {
            return;
        }
        let path = |tree: &Self, mut target: Option<NodeId>| {
            let mut result = Vec::new();
            while let Some(id) = target {
                result.push(id);
                target = tree.nodes.get(&id).and_then(|node| node.parent);
            }
            result
        };
        let old = path(self, self.events.hover);
        let new = path(self, hover);
        self.events.hover = hover;
        for id in old.iter().filter(|id| !new.contains(id)) {
            self.direct(*id, WidgetEvent::Input(InputEvent::PointerLeft));
        }
        for id in new.iter().rev().filter(|id| !old.contains(id)) {
            self.direct(*id, WidgetEvent::Input(InputEvent::PointerEntered));
        }
    }
    pub(crate) fn detach_input(&mut self, id: NodeId) {
        if self.events.capture == Some(id) {
            self.set_capture(None);
        }
        if self.events.focus == Some(id) {
            self.set_focus(None);
        }
        if self.events.hover == Some(id) {
            self.set_hover(None);
        }
    }
    pub(crate) fn prune_input(&mut self) {
        if self
            .events
            .capture
            .is_some_and(|id| !self.nodes.get(&id).is_some_and(|n| n.widget.enabled()))
        {
            self.set_capture(None);
        }
        if self.events.focus.is_some_and(|id| {
            !self
                .nodes
                .get(&id)
                .is_some_and(|n| n.widget.enabled() && n.widget.focusable())
        }) {
            self.set_focus(None);
        }
    }
}
