//! Font-independent monochrome icons. Paths use a 24×24 design coordinate system.
use alloc::sync::Arc;
use zui_api::{
    layout::{AvailableSpace, LayoutError},
    view::ViewSpec,
    widget::*,
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Affine, Color, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    ChevronDown,
    ChevronRight,
    ChevronUp,
    ChevronLeft,
    Check,
    Close,
    Plus,
    Minus,
}

/// Immutable, shareable geometry. Applications can retain icons in their own registry.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    path: Arc<Path>,
    stroke_width: Option<f64>,
}
impl Icon {
    /// Coordinates are in a 24×24 view box. None fills the path; Some strokes it.
    pub fn custom(path: Path, stroke_width: Option<f64>) -> Option<Self> {
        if !path.is_valid() || stroke_width.is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 24.0)
        {
            return None;
        }
        Some(Self {
            path: Arc::new(path),
            stroke_width,
        })
    }
    pub fn builtin(kind: IconKind) -> Self {
        let segments: &[&[(f64, f64)]] = match kind {
            IconKind::ChevronDown => &[&[(6., 9.), (12., 15.), (18., 9.)]],
            IconKind::ChevronRight => &[&[(9., 6.), (15., 12.), (9., 18.)]],
            IconKind::ChevronUp => &[&[(6., 15.), (12., 9.), (18., 15.)]],
            IconKind::ChevronLeft => &[&[(15., 6.), (9., 12.), (15., 18.)]],
            IconKind::Check => &[&[(5., 12.), (10., 17.), (19., 7.)]],
            IconKind::Close => &[&[(6., 6.), (18., 18.)], &[(18., 6.), (6., 18.)]],
            IconKind::Plus => &[&[(5., 12.), (19., 12.)], &[(12., 5.), (12., 19.)]],
            IconKind::Minus => &[&[(5., 12.), (19., 12.)]],
        };
        let mut path = Path::new();
        for segment in segments {
            path.move_to(Point::new(segment[0].0, segment[0].1));
            for &(x, y) in &segment[1..] {
                path.line_to(Point::new(x, y));
            }
        }
        Self {
            path: Arc::new(path),
            stroke_width: Some(2.0),
        }
    }
    /// Aspect-preserving, centered and clipped; no font or backend dependency.
    pub fn paint(&self, bounds: Rect, color: Color, painter: &mut Painter<'_>) {
        if !bounds.is_valid() || bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return;
        }
        let side = bounds.size.width.min(bounds.size.height);
        let transform = Affine::translate(
            bounds.origin.x + (bounds.size.width - side) / 2.0,
            bounds.origin.y + (bounds.size.height - side) / 2.0,
        )
        .compose(Affine::scale(side / 24.0, side / 24.0));
        painter.with_clip(Path::rectangle(bounds), |p| {
            p.with_transform(transform, |p| {
                if let Some(width) = self.stroke_width {
                    p.stroke((*self.path).clone(), color, width);
                } else {
                    p.fill((*self.path).clone(), color);
                }
            })
        });
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct IconView {
    pub icon: Icon,
    pub size: f64,
    pub color: Color,
}
impl IconView {
    pub fn themed(theme: &Theme, icon: Icon) -> Self {
        Self {
            icon,
            size: theme.typography(TypeRole::Body).font_size() as f64,
            color: theme.color(ColorRole::Text),
        }
    }
    fn validate(&self, id: zui_types::NodeId) -> Result<(), LayoutError> {
        if !self.size.is_finite() || self.size < 0.0 || self.size > f32::MAX as f64 {
            Err(LayoutError::InvalidStyle(id))
        } else {
            Ok(())
        }
    }
}
pub struct IconWidget {
    properties: IconView,
}
impl ViewSpec for IconView {
    type Widget = IconWidget;
    fn build(&self) -> IconWidget {
        IconWidget {
            properties: self.clone(),
        }
    }
    fn update(&self, w: &mut IconWidget) -> Invalidation {
        let result = if self.size != w.properties.size {
            Invalidation::Layout
        } else if self != &w.properties {
            Invalidation::Paint
        } else {
            Invalidation::None
        };
        w.properties = self.clone();
        result
    }
}
impl Widget for IconWidget {
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.validate(c.id)?;
        let axis = |known: Option<f32>, available| {
            known.map(f64::from).unwrap_or_else(|| match available {
                AvailableSpace::Definite(v) => self.properties.size.min(v as f64),
                AvailableSpace::MinContent => 0.0,
                AvailableSpace::MaxContent => self.properties.size,
            })
        };
        Ok(Size::new(
            axis(c.input.known_width, c.input.available_width),
            axis(c.input.known_height, c.input.available_height),
        ))
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.properties.validate(c.id)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.properties
            .icon
            .paint(c.content_bounds, self.properties.color, p);
    }
}
