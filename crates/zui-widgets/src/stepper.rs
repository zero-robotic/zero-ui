//! Controlled numeric stepping composed from ordinary buttons and text.
use crate::{Button, ButtonActivated, ButtonStyle, ButtonView, NumericRange, TextView, row};
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::{EventContext, EventResponse, WidgetEvent},
    layout::{Align, Edges, LayoutError, LayoutStyle},
    text::TextStyle,
    view::{Key, View, ViewSpec},
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::Painter;
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepperChanged {
    pub value: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidStepperValue;
impl core::fmt::Display for InvalidStepperValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("stepper value must be finite and inside its range")
    }
}
impl core::error::Error for InvalidStepperValue {}

#[derive(Clone, Debug)]
pub struct StepperView {
    pub value: f64,
    pub range: NumericRange,
    pub disabled: bool,
    pub action: ActionId,
    /// Optional application-formatted value/unit. Never parsed as input.
    pub display: Option<String>,
    pub text_style: TextStyle,
    pub text_color: Color,
    pub button_style: ButtonStyle,
    pub layout: LayoutStyle,
    pub button_layout: LayoutStyle,
    pub value_layout: LayoutStyle,
    pub key: Option<Key>,
}
impl StepperView {
    pub fn themed(
        theme: &Theme,
        families: Vec<String>,
        value: f64,
        range: NumericRange,
        action: ActionId,
    ) -> Self {
        let button = ButtonView::themed(theme, "", families, action);
        Self {
            value,
            range,
            action,
            disabled: false,
            display: None,
            text_style: button.text_style,
            text_color: theme.color(ColorRole::Text),
            button_style: button.style,
            layout: LayoutStyle {
                align: Align::Center,
                gap: Size::new(8.0, 0.0),
                ..Default::default()
            },
            button_layout: LayoutStyle {
                padding: Edges::all(8.0),
                shrink: 0.0,
                ..Default::default()
            },
            value_layout: LayoutStyle::default(),
            key: None,
        }
    }
    pub fn build(self) -> Result<View, InvalidStepperValue> {
        if !self.range.contains(self.value) {
            return Err(InvalidStepperValue);
        }
        let button = |forward, label: &str, key: &str| {
            let value = self.range.shift(self.value, forward, 1);
            View::new(StepButtonView {
                button: ButtonView {
                    trailing_icon: None,
                    label: label.into(),
                    text_style: self.text_style.clone(),
                    style: self.button_style,
                    action: self.action,
                    disabled: self.disabled || value == self.value,
                },
                source: self.value,
                range: self.range,
                value,
            })
            .keyed(key)
            .styled(self.button_layout.clone())
        };
        let children = vec![
            button(false, "−", "decrement"),
            View::new(TextView::new(
                self.display
                    .clone()
                    .unwrap_or_else(|| self.value.to_string()),
                self.text_style.clone(),
                self.text_color,
            ))
            .keyed("value")
            .styled(self.value_layout.clone()),
            button(true, "+", "increment"),
        ];
        let mut root = row(children).style(|s| *s = self.layout).build();
        root.key = self.key;
        Ok(root)
    }
}
struct StepButtonView {
    button: ButtonView,
    source: f64,
    range: NumericRange,
    value: f64,
}
struct StepButton {
    button: Button,
    source: f64,
    range: NumericRange,
    value: f64,
}
impl ViewSpec for StepButtonView {
    type Widget = StepButton;
    fn build(&self) -> StepButton {
        StepButton {
            button: self.button.build(),
            source: self.source,
            range: self.range,
            value: self.value,
        }
    }
    fn update(&self, widget: &mut StepButton) -> Invalidation {
        let changed = self.source != widget.source || self.range != widget.range;
        if changed {
            widget.button.cancel_press();
        }
        widget.source = self.source;
        widget.range = self.range;
        widget.value = self.value;
        let invalidation = self.button.update(&mut widget.button);
        if changed {
            invalidation.max(Invalidation::Paint)
        } else {
            invalidation
        }
    }
}
impl Widget for StepButton {
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
                *action = WidgetAction::new(action.id, StepperChanged { value: self.value });
            }
        }
        response
    }
}
