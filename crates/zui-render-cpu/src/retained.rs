use crate::CpuRenderer;
use zui_paint::{DamageConfig, DamageStats, Frame, PixelBuffer, RenderError, RenderLimits, Scene};
use zui_vello::damage::{DamagePlan, DamageTracker, PixelRegion};

pub struct RetainedCpuRenderer {
    renderer: CpuRenderer,
    limits: RenderLimits,
    config: DamageConfig,
    pixels: Option<PixelBuffer>,
    scratch: Option<PixelBuffer>,
    damage: DamageTracker,
}

impl Default for RetainedCpuRenderer {
    fn default() -> Self {
        Self::new(RenderLimits::default())
    }
}

impl RetainedCpuRenderer {
    pub fn new(limits: RenderLimits) -> Self {
        Self::with_damage_config(limits, DamageConfig::default())
    }
    pub fn with_damage_config(limits: RenderLimits, config: DamageConfig) -> Self {
        Self {
            renderer: CpuRenderer::new(limits),
            limits,
            config,
            pixels: None,
            scratch: None,
            damage: DamageTracker::new(config),
        }
    }
    pub fn reused_frame(&self) -> bool {
        self.damage.stats().reused_frame
    }
    pub fn damage_stats(&self) -> DamageStats {
        self.damage.stats()
    }
    pub fn invalidate(&mut self) {
        self.damage.invalidate();
    }
    pub fn render(&mut self, scene: &Scene, frame: Frame) -> Result<&PixelBuffer, RenderError> {
        if let Err(error) = self.update(scene, frame) {
            self.invalidate();
            return Err(error);
        }
        Ok(self.pixels.as_ref().expect("successful render has target"))
    }
    fn update(&mut self, scene: &Scene, frame: Frame) -> Result<(), RenderError> {
        zui_vello::dimensions(frame, self.limits)?;
        scene.validate(self.limits)?;
        let bytes = frame.extent.rgba_len().ok_or(RenderError::TargetTooLarge)?;
        let cache_pixels = bytes.saturating_mul(2) <= self.config.max_pixel_bytes;
        if !cache_pixels {
            self.damage.invalidate();
            self.scratch = None;
        }
        let plan = if cache_pixels {
            self.damage.plan(scene, frame)?
        } else {
            DamagePlan::full(frame)
        };
        if self
            .pixels
            .as_ref()
            .is_none_or(|pixels| pixels.extent() != frame.extent)
        {
            self.scratch = None;
            self.pixels = Some(PixelBuffer::new(frame.extent, self.limits)?);
        }
        for region in &plan.regions {
            if *region == PixelRegion::full(frame) {
                self.renderer
                    .render_into(scene, frame, self.pixels.as_mut().unwrap())?;
            } else {
                let extent = zui_types::Extent::new(region.width, region.height);
                if self
                    .scratch
                    .as_ref()
                    .is_none_or(|pixels| pixels.extent() != extent)
                {
                    self.scratch = Some(PixelBuffer::new(extent, self.limits)?);
                }
                let scratch = self.scratch.as_mut().unwrap();
                self.renderer
                    .render_region_into(scene, frame, *region, scratch, Some(&plan))?;
                let target = self.pixels.as_mut().unwrap();
                let stride = target.stride();
                for (row, source) in scratch.pixels().chunks_exact(scratch.stride()).enumerate() {
                    let start = (region.y as usize + row) * stride + region.x as usize * 4;
                    let destination = &mut target.pixels_mut()[start..start + source.len()];
                    if destination != source {
                        destination.copy_from_slice(source);
                    }
                }
            }
        }
        let scratch_bytes = self
            .scratch
            .as_ref()
            .map_or(0, |pixels| pixels.pixels().len());
        self.damage
            .commit(plan, frame, bytes.saturating_add(scratch_bytes));
        Ok(())
    }
}
