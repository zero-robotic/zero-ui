//! Passive custom drawing without implementing a complete Widget.
use alloc::rc::Rc;
use zui_api::{
    layout::{AvailableSpace, LayoutError},
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_types::{NodeId, Point, Size};
type Draw = dyn Fn(PaintContext, &mut Painter<'_>);

/// Clone shares the draw callback. A new callback invalidates paint; callers
/// using interior mutable captured data must change revision when it changes.
#[derive(Clone)]
pub struct CanvasView {
    pub natural_size: Size,
    pub clip: bool,
    pub revision: u64,
    draw: Rc<Draw>,
}
impl CanvasView {
    /// Drawing must be side-effect-free: no model mutation, I/O or scheduling.
    /// This is a passive canvas, not an input/animation/lifecycle callback.
    pub fn new(
        natural_size: Size,
        draw: impl Fn(PaintContext, &mut Painter<'_>) + 'static,
    ) -> Self {
        Self {
            natural_size,
            clip: true,
            revision: 0,
            draw: Rc::new(draw),
        }
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        let size = self.natural_size;
        if !size.is_valid() || size.width > f32::MAX as f64 || size.height > f32::MAX as f64 {
            Err(LayoutError::InvalidStyle(id))
        } else {
            Ok(())
        }
    }
}
pub struct Canvas {
    properties: CanvasView,
}
impl ViewSpec for CanvasView {
    type Widget = Canvas;
    fn build(&self) -> Canvas {
        Canvas {
            properties: self.clone(),
        }
    }
    fn update(&self, widget: &mut Canvas) -> Invalidation {
        let old = &widget.properties;
        let invalidation = if self.natural_size != old.natural_size {
            Invalidation::Layout
        } else if self.clip != old.clip
            || self.revision != old.revision
            || !Rc::ptr_eq(&self.draw, &old.draw)
        {
            Invalidation::Paint
        } else {
            Invalidation::None
        };
        widget.properties = self.clone();
        invalidation
    }
}
impl Widget for Canvas {
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
        let input = context.input;
        Ok(Size::new(
            input.known_width.map(f64::from).unwrap_or_else(|| {
                preferred(self.properties.natural_size.width, input.available_width)
            }),
            input.known_height.map(f64::from).unwrap_or_else(|| {
                preferred(self.properties.natural_size.height, input.available_height)
            }),
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.properties.validate(context.id)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        if context.content_bounds.size.width <= 0.0 || context.content_bounds.size.height <= 0.0 {
            return;
        }
        if self.properties.clip {
            painter.with_clip(Path::rectangle(context.content_bounds), |p| {
                (self.properties.draw)(context, p)
            });
        } else {
            (self.properties.draw)(context, painter);
        }
    }
}
