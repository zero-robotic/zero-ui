//! Internal composition adapter. Core transports the payload without knowing T.
use crate::{Button, ButtonActivated, ButtonView};
use zui_api::{action::WidgetAction, event::*, layout::LayoutError, view::ViewSpec, widget::*};
use zui_paint::Painter;
use zui_types::Size;
#[derive(Clone)]
pub(crate) struct ActionButtonView<T: Clone + PartialEq + 'static> {
    pub button: ButtonView,
    pub payload: T,
}
pub(crate) struct ActionButton<T: Clone + PartialEq + 'static> {
    button: Button,
    payload: T,
}
impl<T: Clone + PartialEq + 'static> ViewSpec for ActionButtonView<T> {
    type Widget = ActionButton<T>;
    fn build(&self) -> Self::Widget {
        ActionButton {
            button: self.button.build(),
            payload: self.payload.clone(),
        }
    }
    fn update(&self, w: &mut Self::Widget) -> Invalidation {
        let changed = w.payload != self.payload;
        if changed {
            w.button.cancel_press();
        }
        w.payload = self.payload.clone();
        self.button.update(&mut w.button).max(if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        })
    }
}
impl<T: Clone + PartialEq + 'static> Widget for ActionButton<T> {
    fn enabled(&self) -> bool {
        self.button.enabled()
    }
    fn focusable(&self) -> bool {
        self.button.focusable()
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.button.measure(c)
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.button.prepare(c)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.button.paint(c, p)
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        let mut r = self.button.event(c, e);
        for a in &mut r.actions {
            if a.payload::<ButtonActivated>().is_some() {
                *a = WidgetAction::new(a.id, self.payload.clone());
            }
        }
        r
    }
}
