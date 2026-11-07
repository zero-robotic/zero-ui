//! An independently compiled custom widget: no core, layout engine or backend dependency.
#![no_std]
use zui_api::{
    layout::LayoutError,
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, Widget},
};
use zui_paint::Painter;
use zui_types::{Color, Rect, Size};

/// Minimal custom meter; text, focus and accessibility are separate later stages.
pub struct Meter {
    value: f64,
    foreground: Color,
    background: Color,
}

/// Validated declarative properties, separate from the persistent widget instance.
#[derive(Clone, Copy)]
pub struct MeterView {
    value: f64,
    foreground: Color,
    background: Color,
}
impl MeterView {
    pub fn new(value: f64, foreground: Color, background: Color) -> Result<Self, InvalidValue> {
        validate(value)?;
        Ok(Self {
            value,
            foreground,
            background,
        })
    }
}
impl ViewSpec for MeterView {
    type Widget = Meter;
    fn build(&self) -> Meter {
        Meter {
            value: self.value,
            foreground: self.foreground,
            background: self.background,
        }
    }
    fn update(&self, widget: &mut Meter) -> Invalidation {
        if widget.value == self.value
            && widget.foreground == self.foreground
            && widget.background == self.background
        {
            return Invalidation::None;
        }
        widget.value = self.value;
        widget.foreground = self.foreground;
        widget.background = self.background;
        Invalidation::Paint
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidValue;
impl core::fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("meter value must be finite and between zero and one")
    }
}
impl core::error::Error for InvalidValue {}

impl Meter {
    pub fn new(value: f64, foreground: Color, background: Color) -> Result<Self, InvalidValue> {
        validate(value)?;
        Ok(Self {
            value,
            foreground,
            background,
        })
    }
    /// A value change only needs paint invalidation, never layout invalidation.
    pub fn set_value(&mut self, value: f64) -> Result<(), InvalidValue> {
        validate(value)?;
        self.value = value;
        Ok(())
    }
    pub fn value(&self) -> f64 {
        self.value
    }
}
fn validate(value: f64) -> Result<(), InvalidValue> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(InvalidValue)
    }
}
impl Widget for Meter {
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        // This widget scales its drawing into the assigned box. Its measured
        // content must not report phantom overflow beyond a definite constraint.
        let preferred = |natural: f32, space| match space {
            zui_api::layout::AvailableSpace::Definite(limit) => natural.min(limit),
            zui_api::layout::AvailableSpace::MinContent => 0.0,
            zui_api::layout::AvailableSpace::MaxContent => natural,
        };
        Ok(Size::new(
            context
                .input
                .known_width
                .unwrap_or_else(|| preferred(160.0, context.input.available_width))
                as f64,
            context
                .input
                .known_height
                .unwrap_or_else(|| preferred(32.0, context.input.available_height))
                as f64,
        ))
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let size = context.size;
        painter.fill_rect(
            Rect::new(0.0, 0.0, size.width, size.height),
            self.background,
        );
        if self.value > 0.0 {
            painter.fill_rect(
                Rect::new(0.0, 0.0, size.width * self.value, size.height),
                self.foreground,
            );
        }
    }
}
