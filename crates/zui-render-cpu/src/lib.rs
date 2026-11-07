//! Vello CPU adapter for ZUI's portable drawing protocol.
mod images;
use vello_cpu::{PixmapMut, RenderContext, Resources};
use zui_paint::{Frame, OffscreenRenderer, PixelBuffer, RenderError, RenderLimits, Scene};
use zui_vello::{
    Canvas,
    kurbo::{Affine, BezPath, Stroke},
    peniko::Color,
};

pub struct CpuRenderer {
    context: RenderContext,
    resources: Resources,
    limits: RenderLimits,
    fonts: zui_vello::FontCache,
    images: images::Images,
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new(RenderLimits::default())
    }
}

impl CpuRenderer {
    pub fn new(limits: RenderLimits) -> Self {
        Self {
            context: RenderContext::new(1, 1),
            resources: Resources::new(),
            limits,
            fonts: zui_vello::FontCache::default(),
            images: images::Images::default(),
        }
    }

    /// Reuses a caller-owned pixel buffer; useful for display driver integration.
    pub fn render_into(
        &mut self,
        scene: &Scene,
        frame: Frame,
        target: &mut PixelBuffer,
    ) -> Result<(), RenderError> {
        let (width, height) = zui_vello::dimensions(frame, self.limits)?;
        scene.validate(self.limits)?;
        if target.extent() != frame.extent {
            return Err(RenderError::InvalidTarget);
        }
        self.context.reset_and_resize(width, height);
        self.images.prepare(scene);
        zui_vello::replay(
            scene,
            frame,
            &mut self.fonts,
            &mut CpuCanvas(&mut self.context, &mut self.resources, &mut self.images),
        )?;
        self.context.flush();
        let pixmap =
            PixmapMut::new(width, height, target.pixels_mut()).ok_or(RenderError::InvalidTarget)?;
        self.context.render(pixmap, &mut self.resources);
        Ok(())
    }
}

impl OffscreenRenderer for CpuRenderer {
    fn render(&mut self, scene: &Scene, frame: Frame) -> Result<PixelBuffer, RenderError> {
        zui_vello::dimensions(frame, self.limits)?;
        scene.validate(self.limits)?;
        let mut target = PixelBuffer::new(frame.extent, self.limits)?;
        self.render_into(scene, frame, &mut target)?;
        Ok(target)
    }
}

struct CpuCanvas<'a>(
    &'a mut RenderContext,
    &'a mut Resources,
    &'a mut images::Images,
);
impl Canvas for CpuCanvas<'_> {
    fn image(
        &mut self,
        image: &zui_paint::ImageResource,
        sampling: zui_paint::ImageSampling,
    ) -> Result<(), RenderError> {
        use zui_vello::kurbo::Shape;
        let pixmap = self.2.get(image)?;
        self.0.set_paint(vello_cpu::Image {
            image: vello_cpu::ImageSource::Pixmap(pixmap),
            sampler: zui_vello::image_sampler(sampling),
        });
        let e = image.extent();
        self.0.fill_path(
            &zui_vello::kurbo::Rect::new(0.0, 0.0, e.width as f64, e.height as f64).to_path(0.1),
        );
        Ok(())
    }
    fn glyphs(
        &mut self,
        run: &zui_paint::GlyphRun,
        font: &zui_vello::peniko::FontData,
        color: Color,
    ) -> Result<(), RenderError> {
        self.0.set_paint(color);
        self.0
            .glyph_run(self.1, font)
            .font_size(run.font_size)
            .hint(false)
            .normalized_coords(zui_vello::coords(run))
            .fill_glyphs(zui_vello::glyphs(run))
            .map_err(|error| RenderError::Backend(error.to_string()))
    }
    fn transform(&mut self, transform: Affine) {
        self.0.set_transform(transform);
    }
    fn fill(&mut self, path: &BezPath, color: Color) {
        self.0.set_paint(color);
        self.0.fill_path(path);
    }
    fn stroke(&mut self, path: &BezPath, color: Color, stroke: Stroke) {
        self.0.set_stroke(stroke);
        self.0.set_paint(color);
        self.0.stroke_path(path);
    }
    fn push_clip(&mut self, path: &BezPath) {
        self.0.push_clip_path(path);
    }
    fn pop_clip(&mut self) {
        self.0.pop_clip();
    }
}
