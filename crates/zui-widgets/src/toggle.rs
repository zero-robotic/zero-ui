//! A controlled two-state button using the ordinary button's interaction engine.
use crate::{Button, ButtonActivated, ButtonColors, ButtonStyle, ButtonView};
use alloc::{string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::{EventContext, EventResponse, WidgetEvent},
    layout::LayoutError,
    text::TextStyle,
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::Painter;
use zui_theme::{ColorRole, Theme};
use zui_types::Size;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleChanged {
    pub selected: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleButtonStyle {
    pub unselected: ButtonStyle,
    pub selected: ButtonStyle,
}
impl ToggleButtonStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let selected = ButtonStyle::from_theme(theme);
        let normal = ButtonColors {
            background: theme.color(ColorRole::Surface),
            foreground: theme.color(ColorRole::Text),
        };
        Self {
            selected,
            unselected: ButtonStyle {
                normal,
                hovered: ButtonColors {
                    background: theme.color(ColorRole::SurfaceRaised),
                    ..normal
                },
                pressed: ButtonColors {
                    background: theme.color(ColorRole::Border),
                    ..normal
                },
                disabled: ButtonColors {
                    foreground: theme.color(ColorRole::TextMuted),
                    ..normal
                },
                focus_ring: theme.color(ColorRole::Accent),
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ToggleButtonView {
    pub label: String,
    pub selected: bool,
    pub disabled: bool,
    pub action: ActionId,
    pub text_style: TextStyle,
    pub style: ToggleButtonStyle,
}
impl ToggleButtonView {
    pub fn themed(
        theme: &Theme,
        label: impl Into<String>,
        families: Vec<String>,
        selected: bool,
        action: ActionId,
    ) -> Self {
        let button = ButtonView::themed(theme, label, families, action);
        Self {
            label: button.label,
            selected,
            disabled: false,
            action,
            text_style: button.text_style,
            style: ToggleButtonStyle::from_theme(theme),
        }
    }
    fn button(&self) -> ButtonView {
        ButtonView {
            trailing_icon: None,
            label: self.label.clone(),
            text_style: self.text_style.clone(),
            style: if self.selected {
                self.style.selected
            } else {
                self.style.unselected
            },
            action: self.action,
            disabled: self.disabled,
        }
    }
}
pub struct ToggleButton {
    button: Button,
    selected: bool,
}
impl ViewSpec for ToggleButtonView {
    type Widget = ToggleButton;
    fn build(&self) -> ToggleButton {
        ToggleButton {
            button: self.button().build(),
            selected: self.selected,
        }
    }
    fn update(&self, widget: &mut ToggleButton) -> Invalidation {
        let changed = self.selected != widget.selected;
        if changed {
            widget.button.cancel_press();
        }
        widget.selected = self.selected;
        let invalidation = self.button().update(&mut widget.button);
        if changed {
            invalidation.max(Invalidation::Paint)
        } else {
            invalidation
        }
    }
}
impl Widget for ToggleButton {
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
        let mut response = self.button.event(context, event);
        for action in &mut response.actions {
            if action.payload::<ButtonActivated>().is_some() {
                *action = WidgetAction::new(
                    action.id,
                    ToggleChanged {
                        selected: !self.selected,
                    },
                );
            }
        }
        response
    }
}
