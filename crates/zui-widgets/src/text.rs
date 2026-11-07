use alloc::{string::String, vec::Vec};
use zui_api::{
    layout::{AvailableSpace, LayoutError},
    text::{TextLayout, TextRequest, TextStyle, TextWidth},
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::Painter;
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Affine, Color, Size};

/// Plain, wrapping, read-only text. Editing/selection belong to separate controls.
pub struct Text {
    properties: TextView,
    prepared: Option<TextLayout>,
    prepared_key: Option<(TextWidth, u64)>,
    measured: Option<((TextWidth, u64), TextLayout)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextView {
    pub text: String,
    pub style: TextStyle,
    pub color: Color,
}
impl TextView {
    /// Resolves semantic values now; rebuild with the new theme to apply changes.
    /// Font registration and family selection remain explicit caller responsibilities.
    pub fn themed(
        theme: &Theme,
        text: impl Into<String>,
        families: Vec<String>,
        role: TypeRole,
    ) -> Self {
        let metrics = theme.typography(role);
        Self::new(
            text,
            TextStyle {
                families,
                font_size: metrics.font_size(),
                line_height: metrics.line_height(),
                align: Default::default(),
            },
            theme.color(ColorRole::Text),
        )
    }
    pub fn new(text: impl Into<String>, style: TextStyle, color: Color) -> Self {
        Self {
            text: text.into(),
            style,
            color,
        }
    }
}
impl ViewSpec for TextView {
    type Widget = Text;
    fn build(&self) -> Text {
        Text {
            properties: self.clone(),
            prepared: None,
            prepared_key: None,
            measured: None,
        }
    }
    fn update(&self, widget: &mut Text) -> Invalidation {
        let invalidation =
            if self.text != widget.properties.text || self.style != widget.properties.style {
                widget.prepared = None;
                widget.prepared_key = None;
                widget.measured = None;
                Invalidation::Layout
            } else if self.color != widget.properties.color {
                Invalidation::Paint
            } else {
                Invalidation::None
            };
        widget.properties = self.clone();
        invalidation
    }
}
impl Widget for Text {
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        let input = context.input;
        let width = match (input.known_width, input.available_width) {
            (Some(width), _) | (None, AvailableSpace::Definite(width)) => TextWidth::AtMost(width),
            (None, AvailableSpace::MinContent) => TextWidth::MinContent,
            (None, AvailableSpace::MaxContent) => TextWidth::Unbounded,
        };
        let key = (width, context.text.font_epoch());
        // Two bounded slots: the committed paint layout and the latest measure.
        // Unrelated layout invalidation must not reshape unchanged text.
        let size = if self.prepared_key == Some(key) {
            self.prepared
                .as_ref()
                .expect("prepared key has layout")
                .size
        } else {
            if self.measured.as_ref().map(|(key, _)| *key) != Some(key) {
                self.measured = None;
                let layout = context
                    .text
                    .layout(TextRequest {
                        text: &self.properties.text,
                        style: &self.properties.style,
                        width,
                    })
                    .map_err(|error| LayoutError::Text(context.id, error))?;
                self.measured = Some((key, layout));
            }
            self.measured.as_ref().expect("measured layout").1.size
        };
        Ok(Size::new(
            input.known_width.map(f64::from).unwrap_or(size.width),
            input.known_height.map(f64::from).unwrap_or(size.height),
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        let width = TextWidth::AtMost(context.content_size.width as f32);
        let key = (width, context.text.font_epoch());
        if self.prepared_key == Some(key) {
            return Ok(());
        }
        self.prepared = None;
        self.prepared_key = None;
        let layout = match self.measured.take() {
            Some((measured_key, layout)) if measured_key == key => layout,
            _ => context
                .text
                .layout(TextRequest {
                    text: &self.properties.text,
                    style: &self.properties.style,
                    width,
                })
                .map_err(|error| LayoutError::Text(context.id, error))?,
        };
        self.prepared = Some(layout);
        self.prepared_key = Some(key);
        Ok(())
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        self.paint_color(context, painter, self.properties.color);
    }
}
impl Text {
    pub(crate) fn prepared_size(&self) -> Size {
        self.prepared
            .as_ref()
            .map_or(Size::ZERO, |layout| layout.size)
    }
    pub(crate) fn paint_color(
        &self,
        context: PaintContext,
        painter: &mut Painter<'_>,
        color: Color,
    ) {
        if let Some(layout) = &self.prepared {
            painter.with_transform(
                Affine::translate(
                    context.content_bounds.origin.x,
                    context.content_bounds.origin.y,
                ),
                |painter| layout.paint(painter, color),
            );
        }
    }
}
