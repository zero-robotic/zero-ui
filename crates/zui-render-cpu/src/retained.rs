use crate::CpuRenderer;
use zui_paint::{Frame, PixelBuffer, RenderError, RenderLimits, Scene};

/// CPU presentation target retaining one validated scene and one pixel buffer.
///
/// The output is borrowed read-only, so a cache hit cannot expose caller-modified
/// pixels. Scene commands and all frame parameters must match exactly. This is
/// whole-frame reuse, not dirty-region rendering; changed frames render normally.
/// Use `CpuRenderer` directly when the caller owns a mutable output buffer or
/// retaining an additional scene is undesirable.
pub struct RetainedCpuRenderer {
    renderer: CpuRenderer,
    limits: RenderLimits,
    pixels: Option<PixelBuffer>,
    previous: Option<(Frame, Scene)>,
    reused: bool,
}
impl Default for RetainedCpuRenderer {
    fn default() -> Self {
        Self::new(RenderLimits::default())
    }
}
impl RetainedCpuRenderer {
    pub fn new(limits: RenderLimits) -> Self {
        Self {
            renderer: CpuRenderer::new(limits),
            limits,
            pixels: None,
            previous: None,
            reused: false,
        }
    }
    /// Whether the last successful render reused its pixels without rasterizing.
    /// Reset to false on errors and explicit invalidation.
    pub fn reused_frame(&self) -> bool {
        self.reused
    }
    /// Drop the retained scene key; the next render must rasterize again.
    pub fn invalidate(&mut self) {
        self.previous = None;
        self.reused = false;
    }
    pub fn render(&mut self, scene: &Scene, frame: Frame) -> Result<&PixelBuffer, RenderError> {
        self.reused = false;
        if self
            .previous
            .as_ref()
            .is_some_and(|(old_frame, old_scene)| {
                old_frame.extent == frame.extent
                    && old_frame.scale == frame.scale
                    && old_frame.background == frame.background
                    && old_scene.commands() == scene.commands()
            })
        {
            // Only successful renders install a key; resources are immutable.
            self.reused = true;
        } else {
            // Invalidate before any fallible work, including a partially failed
            // replay. A later retry must never reuse possibly modified pixels.
            self.previous = None;
            zui_vello::dimensions(frame, self.limits)?;
            scene.validate(self.limits)?;
            if !self
                .pixels
                .as_ref()
                .is_some_and(|p| p.extent() == frame.extent)
            {
                self.pixels = Some(PixelBuffer::new(frame.extent, self.limits)?);
            }
            self.renderer.render_into(
                scene,
                frame,
                self.pixels.as_mut().expect("allocated target"),
            )?;
            self.previous = Some((frame, scene.clone()));
        }
        Ok(self.pixels.as_ref().expect("successful render has target"))
    }
}
