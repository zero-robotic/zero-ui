//! Shared single-choice interaction. Callers own geometry, visuals and action types.
use crate::interaction::Interaction;
use zui_api::{event::*, widget::Invalidation};

pub(crate) trait ChoiceItem {
    type Id: Copy + Eq;
    fn id(&self) -> Self::Id;
    fn disabled(&self) -> bool;
}
fn selectable<I: ChoiceItem>(options: &[I], id: I::Id) -> bool {
    options
        .iter()
        .any(|option| option.id() == id && !option.disabled())
}
fn initial<I: ChoiceItem>(options: &[I], selected: Option<I::Id>) -> Option<I::Id> {
    selected.filter(|id| selectable(options, *id)).or_else(|| {
        options
            .iter()
            .find(|option| !option.disabled())
            .map(ChoiceItem::id)
    })
}
pub(crate) struct ChoiceState<Id> {
    pub(crate) interaction: Interaction,
    pub(crate) active: Option<Id>,
    pub(crate) hovered: Option<Id>,
    pub(crate) pressed: Option<Id>,
}
impl<Id: Copy + Eq> ChoiceState<Id> {
    pub(crate) fn new<I: ChoiceItem<Id = Id>>(options: &[I], selected: Option<Id>) -> Self {
        Self {
            interaction: Default::default(),
            active: initial(options, selected),
            hovered: None,
            pressed: None,
        }
    }
    pub(crate) fn update<I: ChoiceItem<Id = Id>>(
        &mut self,
        options: &[I],
        selected: Option<Id>,
        reset_active: bool,
        cancel: bool,
    ) {
        if reset_active || !self.active.is_some_and(|id| selectable(options, id)) {
            self.active = initial(options, selected);
        }
        if cancel {
            self.interaction.cancel();
            self.pressed = None;
            self.hovered = None;
        }
    }
    fn navigate<I: ChoiceItem<Id = Id>>(&self, options: &[I], key: &Key) -> Option<Id> {
        let first = options.iter().position(|option| !option.disabled())?;
        let last = options.iter().rposition(|option| !option.disabled())?;
        let current = options
            .iter()
            .position(|option| Some(option.id()) == self.active && !option.disabled())
            .unwrap_or(first);
        let index = match key {
            Key::Home => first,
            Key::End => last,
            Key::ArrowUp | Key::ArrowLeft => options[..current]
                .iter()
                .rposition(|option| !option.disabled())
                .unwrap_or(last),
            _ => options[current + 1..]
                .iter()
                .position(|option| !option.disabled())
                .map_or(first, |index| current + 1 + index),
        };
        Some(options[index].id())
    }
    /// Commit a validated navigation target and cancel any activation gesture.
    /// Geometry-based controls choose the target; this state owns interaction.
    pub(crate) fn navigate_to<I: ChoiceItem<Id = Id>>(
        &mut self,
        options: &[I],
        target: Id,
        selected: Option<Id>,
    ) -> Option<Id> {
        if !selectable(options, target) {
            return None;
        }
        self.interaction.cancel();
        self.pressed = None;
        self.active = Some(target);
        (Some(target) != selected).then_some(target)
    }
    pub(crate) fn event<I: ChoiceItem<Id = Id>>(
        &mut self,
        context: EventContext,
        event: &WidgetEvent,
        enabled: bool,
        options: &[I],
        selected: Option<Id>,
        hit: Option<Id>,
    ) -> (EventResponse, Option<Id>) {
        let before = (self.active, self.hovered, self.pressed);
        let pointer = matches!(
            event,
            WidgetEvent::Input(InputEvent::PointerMoved(_) | InputEvent::PointerButton { .. })
        );
        if context.phase == EventPhase::Target && pointer {
            self.hovered = hit.filter(|id| enabled && selectable(options, *id));
            self.interaction.hovered = self.hovered.is_some();
            if matches!(
                event,
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                })
            ) {
                self.pressed = self.hovered;
                if self.pressed.is_some() {
                    self.active = self.pressed;
                }
            }
        }
        if matches!(event, WidgetEvent::Input(InputEvent::PointerLeft)) {
            self.hovered = None;
        }
        if matches!(
            event,
            WidgetEvent::FocusChanged(false) | WidgetEvent::PointerCaptureLost
        ) {
            self.pressed = None;
        }
        if matches!(event, WidgetEvent::FocusChanged(true)) && self.pressed.is_none() {
            self.active = initial(options, selected);
        }
        if context.phase == EventPhase::Target
            && enabled
            && self.interaction.focused
            && let WidgetEvent::Input(InputEvent::Key(key)) = event
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.super_key
            && matches!(
                key.key,
                Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Home
                    | Key::End
            )
        {
            let mut response = EventResponse {
                handled: true,
                ..Default::default()
            };
            let mut changed = None;
            if key.pressed {
                response.capture = CaptureRequest::Release;
                if let Some(target) = self.navigate(options, &key.key) {
                    changed = self.navigate_to(options, target, selected);
                }
                response.invalidation = Invalidation::Paint;
            }
            return (response, changed);
        }
        let (mut response, activated) = self.interaction.event(context, event, enabled, false);
        let chosen = if activated {
            if pointer {
                self.pressed.filter(|id| Some(*id) == self.hovered)
            } else {
                self.active
            }
        } else {
            None
        };
        if !self.interaction.mouse {
            self.pressed = None;
        }
        if before != (self.active, self.hovered, self.pressed) {
            response.invalidation = response.invalidation.max(Invalidation::Paint);
        }
        (response, chosen.filter(|id| Some(*id) != selected))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    struct Item(u16, bool);
    impl ChoiceItem for Item {
        type Id = u16;
        fn id(&self) -> u16 {
            self.0
        }
        fn disabled(&self) -> bool {
            self.1
        }
    }
    #[test]
    fn navigation_matches_enabled_order_for_every_disabled_pattern() {
        for mask in 0..16 {
            let options: Vec<_> = (0..4).map(|i| Item(10 + i, mask & (1 << i) != 0)).collect();
            let enabled: Vec<_> = options
                .iter()
                .filter(|item| !item.disabled())
                .map(ChoiceItem::id)
                .collect();
            for active in [None, Some(9), Some(10), Some(11), Some(12), Some(13)] {
                let mut state = ChoiceState::new(&options, None);
                state.active = active;
                for key in [
                    Key::Home,
                    Key::End,
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::ArrowUp,
                    Key::ArrowDown,
                ] {
                    let expected = if enabled.is_empty() {
                        None
                    } else {
                        let current = enabled
                            .iter()
                            .position(|id| Some(*id) == active)
                            .unwrap_or(0);
                        let index = match key {
                            Key::Home => 0,
                            Key::End => enabled.len() - 1,
                            Key::ArrowLeft | Key::ArrowUp => {
                                (current + enabled.len() - 1) % enabled.len()
                            }
                            _ => (current + 1) % enabled.len(),
                        };
                        Some(enabled[index])
                    };
                    assert_eq!(state.navigate(&options, &key), expected);
                }
            }
        }
    }
    #[test]
    fn reconciliation_preserves_available_active_identity_and_cancels_armed_gestures() {
        let mut state = ChoiceState::new(&[Item(1, false), Item(2, false)], Some(1));
        state.active = Some(2);
        state.pressed = Some(2);
        state.hovered = Some(2);
        state.interaction.mouse = true;
        state.update(&[Item(2, false), Item(1, false)], Some(1), false, true);
        assert_eq!(state.active, Some(2));
        assert_eq!(state.pressed, None);
        assert_eq!(state.hovered, None);
        assert!(!state.interaction.mouse);
        state.update(&[Item(2, true), Item(1, false)], Some(2), false, true);
        assert_eq!(state.active, Some(1));
        state.update(&[Item(1, true)], None, true, true);
        assert_eq!(state.active, None);
    }
}
