use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Dip, Rect, Size};
use zui_render::{ClipShape, ImageId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Fill,
    None,
}

/// Displays a renderer-managed raster image.
pub struct Image {
    id: WidgetId,
    image: ImageId,
    intrinsic: Size,
    fit: ImageFit,
    opacity: f32,
    width: Option<Dip>,
    height: Option<Dip>,
    bounds: Rect,
}
impl Image {
    pub fn new(image: ImageId, width: f32, height: f32) -> Self {
        Self {
            id: WidgetId::new(),
            image,
            intrinsic: Size {
                width: Dip(width.max(0.0)),
                height: Dip(height.max(0.0)),
            },
            fit: ImageFit::Contain,
            opacity: 1.0,
            width: None,
            height: None,
            bounds: Rect::default(),
        }
    }
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(Dip(width.max(0.0)));
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(Dip(height.max(0.0)));
        self
    }
    fn destination(&self) -> Rect {
        let target = self.bounds;
        if matches!(self.fit, ImageFit::Fill)
            || self.intrinsic.width.0 == 0.0
            || self.intrinsic.height.0 == 0.0
        {
            return target;
        }
        let sx = target.size.width.0 / self.intrinsic.width.0;
        let sy = target.size.height.0 / self.intrinsic.height.0;
        let scale = match self.fit {
            ImageFit::Contain => sx.min(sy),
            ImageFit::Cover => sx.max(sy),
            ImageFit::None => 1.0,
            ImageFit::Fill => unreachable!(),
        };
        let size = Size {
            width: Dip(self.intrinsic.width.0 * scale),
            height: Dip(self.intrinsic.height.0 * scale),
        };
        Rect {
            origin: zui_core::Point {
                x: Dip(target.origin.x.0 + (target.size.width.0 - size.width.0) / 2.0),
                y: Dip(target.origin.y.0 + (target.size.height.0 - size.height.0) / 2.0),
            },
            size,
        }
    }
}
impl Widget for Image {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let size = constraints.constrain(Size {
            width: self.width.unwrap_or(self.intrinsic.width),
            height: self.height.unwrap_or(self.intrinsic.height),
        });
        self.bounds.size = size;
        size
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn event(&mut self, _: &UiEvent, _: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        let mut node = build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            ctx.draw_image(self.destination(), self.image, self.opacity)
        });
        // `Cover` intentionally overflows its destination before cropping;
        // clipping keeps that paint inside the widget's layout rectangle.
        node.set_clip(Some(ClipShape::Rect(Rect {
            origin: zui_core::Point::default(),
            size: self.bounds.size,
        })));
        node
    }
}
