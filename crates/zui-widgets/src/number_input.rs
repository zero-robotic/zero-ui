//! Numeric editor: transient text stays local; only valid submissions change the model.
use crate::{NumericRange, TextInput, TextInputEvent, TextInputView};
use alloc::{format, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::LayoutError,
    view::ViewSpec,
    widget::*,
};
use zui_paint::Painter;
use zui_theme::Theme;
use zui_types::Size;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberChanged {
    pub value: f64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberRejected {
    pub text: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NumberInputView {
    pub value: f64,
    pub range: NumericRange,
    /// Decimal display precision; validated in 0..=12. Parsing accepts decimal/exponent syntax.
    pub precision: usize,
    pub editor: TextInputView,
}
impl NumberInputView {
    pub fn themed(
        theme: &Theme,
        value: f64,
        range: NumericRange,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            value,
            range,
            precision: 2,
            editor: TextInputView::themed(theme, "", families, action),
        }
    }
    fn description(&self) -> TextInputView {
        let mut input = self.editor.clone();
        input.text = format!("{:.*}", self.precision.min(12), self.value);
        input.draft = None;
        input
    }
}
pub struct NumberInput {
    properties: NumberInputView,
    editor: TextInput,
    revision: u64,
    invalid: bool,
}
impl ViewSpec for NumberInputView {
    type Widget = NumberInput;
    fn build(&self) -> NumberInput {
        NumberInput {
            properties: self.clone(),
            editor: self.description().build(),
            revision: 0,
            invalid: false,
        }
    }
    fn update(&self, w: &mut NumberInput) -> Invalidation {
        let changed = self.value != w.properties.value
            || self.range != w.properties.range
            || self.precision != w.properties.precision;
        if changed {
            w.revision = w.revision.wrapping_add(1);
            w.invalid = false;
        }
        w.properties = self.clone();
        let mut input = self.description();
        input.revision = w.revision;
        input.invalid |= w.invalid;
        input.update(&mut w.editor)
    }
}
impl NumberInput {
    fn valid(&self, id: zui_types::NodeId) -> Result<(), LayoutError> {
        if !self.properties.range.contains(self.properties.value) || self.properties.precision > 12
        {
            Err(LayoutError::InvalidStyle(id))
        } else {
            Ok(())
        }
    }
    fn reset(&mut self) -> Invalidation {
        self.revision = self.revision.wrapping_add(1);
        self.invalid = false;
        let mut input = self.properties.description();
        input.revision = self.revision;
        input.update(&mut self.editor)
    }
    fn handle(
        &mut self,
        c: EventContext,
        e: &WidgetEvent,
        services: &mut EventServices<'_>,
    ) -> EventResponse {
        if c.phase == EventPhase::Target
            && !self.editor.is_composing()
            && self.enabled()
            && let WidgetEvent::Input(InputEvent::Key(key)) = e
            && key.pressed
            && !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.super_key
        {
            if key.key == Key::Escape
                && (self.invalid
                    || self.editor.text() != Some(self.properties.description().text.as_str()))
            {
                return EventResponse {
                    handled: true,
                    stop_propagation: true,
                    invalidation: self.reset(),
                    ..Default::default()
                };
            }
            if !self.properties.editor.read_only && matches!(key.key, Key::ArrowUp | Key::ArrowDown)
            {
                let value = self
                    .editor
                    .text()
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|v| self.properties.range.contains(*v))
                    .unwrap_or(self.properties.value);
                let value = self.properties.range.shift(
                    value,
                    key.key == Key::ArrowUp,
                    if key.modifiers.shift { 10 } else { 1 },
                );
                return EventResponse {
                    actions: alloc::vec![WidgetAction::new(
                        self.properties.editor.action,
                        NumberChanged { value }
                    )],
                    handled: true,
                    ..Default::default()
                };
            }
        }
        let mut r = self.editor.event_with_services(c, e, services);
        r.actions.retain_mut(|a| {
            let Some(event) = a.payload::<TextInputEvent>() else {
                return true;
            };
            match event {
                TextInputEvent::Changed(text) => {
                    self.invalid = text
                        .parse::<f64>()
                        .ok()
                        .is_none_or(|v| !self.properties.range.contains(v));
                    false
                }
                TextInputEvent::Submitted(text) if !self.properties.editor.read_only => {
                    if let Some(value) = text
                        .parse::<f64>()
                        .ok()
                        .filter(|v| self.properties.range.contains(*v))
                    {
                        *a = WidgetAction::new(a.id, NumberChanged { value });
                        self.invalid = false;
                    } else {
                        *a = WidgetAction::new(a.id, NumberRejected { text: text.clone() });
                        self.invalid = true;
                    }
                    true
                }
                TextInputEvent::Submitted(_) => false,
                TextInputEvent::Rejected(_) => true,
            }
        });
        let mut input = self.properties.description();
        input.revision = self.revision;
        input.invalid |= self.invalid;
        r.invalidation = r.invalidation.max(input.update(&mut self.editor));
        if matches!(e, WidgetEvent::FocusChanged(false)) {
            r.invalidation = r.invalidation.max(self.reset());
        }
        r
    }
}
impl Widget for NumberInput {
    fn enabled(&self) -> bool {
        self.editor.enabled()
    }
    fn focusable(&self) -> bool {
        self.editor.focusable()
    }
    fn text_input(&self, c: PaintContext) -> Option<TextInputState> {
        self.editor.text_input(c)
    }
    fn animate(&mut self, t: core::time::Duration) -> AnimationUpdate {
        self.editor.animate(t)
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.valid(c.id)?;
        self.editor.measure(c)
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.valid(c.id)?;
        self.editor.prepare(c)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.editor.paint(c, p)
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
