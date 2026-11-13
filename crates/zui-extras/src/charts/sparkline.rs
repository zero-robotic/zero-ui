use super::{ChartRange, SparklineSeries, series::fraction};
use zui_api::{
    layout::{AvailableSpace, LayoutError},
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, NodeId, Point, Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SparklineStyle {
    pub line: Color,
    /// Area extends to the bottom of the plot, not the numeric zero value.
    pub area: Option<Color>,
    pub line_width: f64,
    pub natural_size: Size,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SparklineView {
    pub series: SparklineSeries,
    /// None uses the finite sample extrema; constant series are centered.
    pub range: Option<ChartRange>,
    pub style: SparklineStyle,
}
impl SparklineView {
    pub fn themed(theme: &Theme, series: SparklineSeries) -> Self {
        let line = theme.color(ColorRole::Accent);
        Self {
            series,
            range: None,
            style: SparklineStyle {
                line,
                area: Some(Color::rgba(line.r, line.g, line.b, 40)),
                line_width: 2.0,
                natural_size: Size::new(160.0, 48.0),
            },
        }
    }
    fn valid(&self) -> bool {
        let s = self.style;
        [s.line_width, s.natural_size.width, s.natural_size.height]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0 && *v <= f32::MAX as f64)
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        if self.valid() {
            Ok(())
        } else {
            Err(LayoutError::InvalidStyle(id))
        }
    }
}
pub struct Sparkline {
    properties: SparklineView,
}
impl ViewSpec for SparklineView {
    type Widget = Sparkline;
    fn build(&self) -> Sparkline {
        Sparkline {
            properties: self.clone(),
        }
    }
    fn update(&self, widget: &mut Sparkline) -> Invalidation {
        if self == &widget.properties {
            return Invalidation::None;
        }
        let layout = !self.valid()
            || !widget.properties.valid()
            || self.style.natural_size != widget.properties.style.natural_size;
        widget.properties = self.clone();
        if layout {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl Widget for Sparkline {
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.validate(context.id)?;
        let preferred = |size: f64, space| match space {
            AvailableSpace::Definite(limit) => size.min(limit as f64),
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => size,
        };
        Ok(Size::new(
            context.input.known_width.map(f64::from).unwrap_or_else(|| {
                preferred(
                    self.properties.style.natural_size.width,
                    context.input.available_width,
                )
            }),
            context
                .input
                .known_height
                .map(f64::from)
                .unwrap_or_else(|| {
                    preferred(
                        self.properties.style.natural_size.height,
                        context.input.available_height,
                    )
                }),
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.properties.validate(context.id)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let p = &self.properties;
        let bounds = context.content_bounds;
        if !p.valid() || bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return;
        }
        let Some((min, max)) = p.range.map(|r| (r.min(), r.max())).or(p.series.bounds()) else {
            return;
        };
        let values = p.series.values();
        let inset = (p.style.line_width / 2.0)
            .min(bounds.size.width / 2.0)
            .min(bounds.size.height / 2.0);
        let width = (bounds.size.width - inset * 2.0).max(0.0);
        let height = (bounds.size.height - inset * 2.0).max(0.0);
        let bottom = bounds.origin.y + inset + height;
        let point = |index: usize, value| {
            Point::new(
                bounds.origin.x
                    + inset
                    + if values.len() <= 1 {
                        width / 2.0
                    } else {
                        index as f64 / (values.len() - 1) as f64 * width
                    },
                bottom - fraction(value, min, max) * height,
            )
        };
        painter.with_clip(Path::rectangle(bounds), |painter| {
            let mut start = 0;
            while start < values.len() {
                let Some(value) = values[start] else {
                    start += 1;
                    continue;
                };
                let first = point(start, value);
                let mut end = start + 1;
                let mut last = first;
                let mut path = Path::new();
                path.move_to(first);
                while end < values.len() {
                    let Some(value) = values[end] else {
                        break;
                    };
                    last = point(end, value);
                    path.line_to(last);
                    end += 1;
                }
                if end == start + 1 {
                    let size = p.style.line_width;
                    painter.fill_rect(
                        Rect::new(first.x - size / 2.0, first.y - size / 2.0, size, size),
                        p.style.line,
                    );
                } else {
                    if let Some(color) = p.style.area {
                        let mut area = path.clone();
                        area.line_to(Point::new(last.x, bottom));
                        area.line_to(Point::new(first.x, bottom));
                        area.close();
                        painter.fill(area, color);
                    }
                    painter.stroke(path, p.style.line, p.style.line_width);
                }
                start = end;
            }
        });
    }
}
