//! Internal typed-commit adapter shared by semantic text editors.
use crate::{TextInput, TextInputEvent, TextInputView};
use alloc::{rc::Rc, string::String};
use core::time::Duration;
use zui_api::{action::WidgetAction, event::*, layout::LayoutError, view::ViewSpec, widget::*};
use zui_paint::Painter;
use zui_types::Size;
pub(crate) type Parser<T> = Rc<dyn Fn(&str) -> Option<T>>;
#[derive(Clone)]
pub(crate) struct CommitInputView<T: Clone + 'static> {
    pub input: TextInputView,
    pub cancel: Option<T>,
    pub reset_on_blur: bool,
    pub parse: Parser<T>,
}
pub(crate) struct CommitInput<T: Clone + 'static> {
    properties: CommitInputView<T>,
    editor: TextInput,
    revision: u64,
    invalid: bool,
}
impl<T: Clone + 'static> ViewSpec for CommitInputView<T> {
    type Widget = CommitInput<T>;
    fn build(&self) -> Self::Widget {
        CommitInput {
            properties: self.clone(),
            editor: self.input.build(),
            revision: self.input.revision,
            invalid: false,
        }
    }
    fn update(&self, w: &mut Self::Widget) -> Invalidation {
        if self.input.text != w.properties.input.text
            || self.input.revision != w.properties.input.revision
            || self.input.action != w.properties.input.action
        {
            w.revision = w.revision.wrapping_add(1);
            w.invalid = false;
        }
        w.properties = self.clone();
        w.sync()
    }
}
impl<T: Clone + 'static> CommitInput<T> {
    fn sync(&mut self) -> Invalidation {
        let mut input = self.properties.input.clone();
        input.revision = self.revision;
        input.invalid |= self.invalid;
        input.update(&mut self.editor)
    }
    fn reset(&mut self) -> Invalidation {
        self.revision = self.revision.wrapping_add(1);
        self.invalid = false;
        self.sync()
    }
    fn handle(
        &mut self,
        c: EventContext,
        e: &WidgetEvent,
        services: &mut EventServices<'_>,
    ) -> EventResponse {
        if self.editor.enabled()
            && c.phase == EventPhase::Target
            && !self.editor.is_composing()
            && matches!(
                e,
                WidgetEvent::Input(InputEvent::Key(KeyInput {
                    key: Key::Escape,
                    pressed: true,
                    ..
                }))
            )
        {
            return EventResponse {
                handled: true,
                invalidation: self.reset(),
                actions: self
                    .properties
                    .cancel
                    .clone()
                    .map(|event| WidgetAction::new(self.properties.input.action, event))
                    .into_iter()
                    .collect(),
                ..Default::default()
            };
        }
        let mut r = self.editor.event_with_services(c, e, services);
        let mut submitted: Option<String> = None;
        r.actions.retain(|a| match a.payload::<TextInputEvent>() {
            Some(TextInputEvent::Submitted(text)) => {
                submitted = Some(text.clone());
                false
            }
            Some(TextInputEvent::Changed(_)) => false,
            _ => true,
        });
        if let Some(text) = submitted {
            if let Some(value) = (self.properties.parse)(&text) {
                self.invalid = false;
                r.actions
                    .push(WidgetAction::new(self.properties.input.action, value));
            } else {
                self.invalid = true;
            }
            r.invalidation = r.invalidation.max(self.sync());
        }
        if self.properties.reset_on_blur && matches!(e, WidgetEvent::FocusChanged(false)) {
            r.invalidation = r.invalidation.max(self.reset());
        }
        r
    }
}
impl<T: Clone + 'static> Widget for CommitInput<T> {
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
        self.editor.paint(c, p)
    }
    fn text_input(&self, c: PaintContext) -> Option<TextInputState> {
        self.editor.text_input(c)
    }
    fn animate(&mut self, t: Duration) -> AnimationUpdate {
        self.editor.animate(t)
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
