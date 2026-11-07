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
        }
    }
    fn update(&self, widget: &mut Text) -> Invalidation {
        let invalidation =
            if self.text != widget.properties.text || self.style != widget.properties.style {
                widget.prepared = None;
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
        let layout = context
            .text
            .layout(TextRequest {
                text: &self.properties.text,
                style: &self.properties.style,
                width,
            })
            .map_err(|error| LayoutError::Text(context.id, error))?;
        Ok(Size::new(
            input
                .known_width
                .map(f64::from)
                .unwrap_or(layout.size.width),
            input
                .known_height
                .map(f64::from)
                .unwrap_or(layout.size.height),
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.prepared = None;
        let layout = context
            .text
            .layout(TextRequest {
                text: &self.properties.text,
                style: &self.properties.style,
                width: TextWidth::AtMost(context.content_size.width as f32),
            })
            .map_err(|error| LayoutError::Text(context.id, error))?;
        self.prepared = Some(layout);
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
