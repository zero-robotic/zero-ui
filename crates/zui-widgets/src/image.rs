//! Passive image display. Loading, decoding and backend resources stay elsewhere.
use zui_api::{
    layout::{AvailableSpace, LayoutError},
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, Widget},
};
use zui_paint::{ImageResource, ImageSampling, Painter, Path};
use zui_types::{Affine, Point, Size};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    /// Preserve aspect ratio and show the entire image.
    #[default]
    Contain,
    /// Preserve aspect ratio, cropping the overflow.
    Cover,
    /// Independently stretch both axes to the content area.
    Fill,
    /// One source pixel per logical unit, cropping overflow.
    None,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageAlign {
    Start,
    #[default]
    Center,
    End,
}
impl ImageAlign {
    fn offset(self, remaining: f64) -> f64 {
        match self {
            Self::Start => 0.0,
            Self::Center => remaining / 2.0,
            Self::End => remaining,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ImageView {
    pub image: ImageResource,
    pub fit: ImageFit,
    pub horizontal: ImageAlign,
    pub vertical: ImageAlign,
    pub sampling: ImageSampling,
}
impl ImageView {
    pub fn new(image: ImageResource) -> Self {
        Self {
            image,
            fit: ImageFit::Contain,
            horizontal: ImageAlign::Center,
            vertical: ImageAlign::Center,
            sampling: ImageSampling::Bilinear,
        }
    }
}
pub struct Image {
    properties: ImageView,
}
impl ViewSpec for ImageView {
    type Widget = Image;
    fn build(&self) -> Image {
        Image {
            properties: self.clone(),
        }
    }
    fn update(&self, widget: &mut Image) -> Invalidation {
        let invalidation = if self.image.extent() != widget.properties.image.extent() {
            Invalidation::Layout
        } else if self != &widget.properties {
            Invalidation::Paint
        } else {
            Invalidation::None
        };
        widget.properties = self.clone();
        invalidation
    }
}
impl Widget for Image {
    fn preferred_aspect_ratio(&self) -> Option<f32> {
        let e = self.properties.image.extent();
        Some(e.width as f32 / e.height as f32)
    }
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        let e = self.properties.image.extent();
        let (width, height) = (e.width as f64, e.height as f64);
        let input = context.input;
        let limit = |space| match space {
            AvailableSpace::Definite(value) => value as f64,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f64::INFINITY,
        };
        Ok(match (input.known_width, input.known_height) {
            (Some(w), Some(h)) => Size::new(w as f64, h as f64),
            (Some(w), None) => Size::new(
                w as f64,
                (w as f64 * height / width).min(limit(input.available_height)),
            ),
            (None, Some(h)) => Size::new(
                (h as f64 * width / height).min(limit(input.available_width)),
                h as f64,
            ),
            (None, None) => {
                let scale = 1.0f64
                    .min(limit(input.available_width) / width)
                    .min(limit(input.available_height) / height);
                Size::new(width * scale, height * scale)
            }
        })
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let bounds = context.content_bounds;
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return;
        }
        let p = &self.properties;
        let e = p.image.extent();
        let sx = bounds.size.width / e.width as f64;
        let sy = bounds.size.height / e.height as f64;
        let (sx, sy) = match p.fit {
            ImageFit::Contain => (sx.min(sy), sx.min(sy)),
            ImageFit::Cover => (sx.max(sy), sx.max(sy)),
            ImageFit::Fill => (sx, sy),
            ImageFit::None => (1.0, 1.0),
        };
        let x = bounds.origin.x + p.horizontal.offset(bounds.size.width - e.width as f64 * sx);
        let y = bounds.origin.y + p.vertical.offset(bounds.size.height - e.height as f64 * sy);
        painter.with_clip(Path::rectangle(bounds), |painter| {
            painter.with_transform(
                Affine::translate(x, y).compose(Affine::scale(sx, sy)),
                |painter| {
                    painter.image(p.image.clone(), p.sampling);
                },
            );
        });
    }
}
