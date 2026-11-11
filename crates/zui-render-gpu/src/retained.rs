use crate::{GpuRenderer, readback};
use zui_paint::{
    DamageConfig, DamageStats, Frame, OffscreenRenderer, PixelBuffer, RenderError, Scene,
};
use zui_types::Extent;
use zui_vello::damage::{DamageTracker, PixelRegion};

pub(super) struct Retention {
    damage: DamageTracker,
    target: Option<wgpu::Texture>,
    scratch: Option<wgpu::Texture>,
    pipeline: Option<wgpu::RenderPipeline>,
}

impl GpuRenderer {
    pub fn set_damage_config(&mut self, config: DamageConfig) {
        self.retained = None;
        self.damage_config = config;
    }
    pub fn invalidate_retained(&mut self) {
        self.retained = None;
    }
    pub fn damage_stats(&self) -> DamageStats {
        self.retained
            .as_ref()
            .map_or(DamageStats::default(), |retained| retained.damage.stats())
    }
    pub fn render_retained(
        &mut self,
        scene: &Scene,
        frame: Frame,
    ) -> Result<PixelBuffer, RenderError> {
        if let Err(error) = frame.validate(self.limits) {
            self.invalidate_retained();
            return Err(error);
        }
        if frame.extent.rgba_len().unwrap().saturating_mul(2) > self.damage_config.max_pixel_bytes {
            self.retained = None;
            return self.render(scene, frame);
        }
        self.with_retained(scene, frame, |renderer, retained| {
            let encoder = renderer.device.create_command_encoder(&Default::default());
            let mut pixels = readback::copy(
                &renderer.device,
                &renderer.queue,
                encoder,
                retained.target.as_ref().unwrap(),
                frame.extent,
                renderer.limits,
            )?;
            if renderer.target_format == wgpu::TextureFormat::Bgra8Unorm {
                for pixel in pixels.pixels_mut().as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
            }
            Ok(pixels)
        })
    }
    pub(super) fn with_retained<ResultValue>(
        &mut self,
        scene: &Scene,
        frame: Frame,
        finish: impl FnOnce(&mut Self, &mut Retention) -> Result<ResultValue, RenderError>,
    ) -> Result<ResultValue, RenderError> {
        let mut retained = self.retained.take().unwrap_or_else(|| Retention {
            damage: DamageTracker::new(self.damage_config),
            target: None,
            scratch: None,
            pipeline: None,
        });
        let result = retained
            .update(self, scene, frame)
            .and_then(|()| finish(self, &mut retained));
        if result.is_err() {
            retained.damage.invalidate();
        }
        self.retained = Some(retained);
        result
    }
}

impl Retention {
    fn update(
        &mut self,
        renderer: &mut GpuRenderer,
        scene: &Scene,
        frame: Frame,
    ) -> Result<(), RenderError> {
        zui_vello::dimensions(frame, renderer.limits)?;
        scene.validate(renderer.limits)?;
        let maximum = renderer.device.limits().max_texture_dimension_2d;
        if frame.extent.width > maximum || frame.extent.height > maximum {
            return Err(RenderError::TargetTooLarge);
        }
        let plan = self.damage.plan(scene, frame)?;
        if self.target.as_ref().is_none_or(|texture| {
            texture.width() != frame.extent.width || texture.height() != frame.extent.height
        }) {
            self.scratch = None;
        }
        ensure_texture(&mut self.target, renderer, frame.extent);
        for region in &plan.regions {
            let (width, height) = renderer.prepare_region(
                scene,
                frame,
                *region,
                (*region != PixelRegion::full(frame)).then_some(&plan),
            )?;
            #[cfg(feature = "profiling")]
            let timing_view = renderer.timing.as_ref().map(|_| {
                self.target
                    .as_ref()
                    .unwrap()
                    .create_view(&Default::default())
            });
            #[cfg(not(feature = "profiling"))]
            let timing_view: Option<wgpu::TextureView> = None;
            let mut encoder = renderer.begin_submission(timing_view.as_ref());
            if *region == PixelRegion::full(frame) {
                let view = self
                    .target
                    .as_ref()
                    .unwrap()
                    .create_view(&Default::default());
                renderer.encode(&mut encoder, &view, width, height)?;
            } else {
                ensure_texture(
                    &mut self.scratch,
                    renderer,
                    Extent::new(region.width, region.height),
                );
                let scratch = self.scratch.as_ref().unwrap();
                let view = scratch.create_view(&Default::default());
                renderer.encode(&mut encoder, &view, width, height)?;
                encoder.copy_texture_to_texture(
                    scratch.as_image_copy(),
                    wgpu::TexelCopyTextureInfo {
                        origin: wgpu::Origin3d {
                            x: region.x,
                            y: region.y,
                            z: 0,
                        },
                        ..self.target.as_ref().unwrap().as_image_copy()
                    },
                    wgpu::Extent3d {
                        width: region.width,
                        height: region.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            renderer.end_submission(encoder, timing_view.as_ref());
        }
        let scratch_bytes = self.scratch.as_ref().map_or(0, |texture| {
            texture.width() as usize * texture.height() as usize * 4
        });
        self.damage.commit(
            plan,
            frame,
            frame
                .extent
                .rgba_len()
                .unwrap()
                .saturating_add(scratch_bytes),
        );
        Ok(())
    }
    pub fn present(
        &mut self,
        renderer: &GpuRenderer,
        view: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        let pipeline = self.pipeline.get_or_insert_with(|| {
            let shader = renderer
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("ZUI retained surface copy"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("retained.wgsl").into()),
                });
            renderer
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("ZUI retained surface copy"),
                    layout: None,
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vertex"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fragment"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: renderer.target_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
        });
        let source = self
            .target
            .as_ref()
            .unwrap()
            .create_view(&Default::default());
        let group = renderer
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ZUI retained surface"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                }],
            });
        let mut encoder = renderer.begin_submission(Some(view));
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        renderer.end_submission(encoder, Some(view));
        Ok(())
    }
}

fn ensure_texture(texture: &mut Option<wgpu::Texture>, renderer: &GpuRenderer, extent: Extent) {
    if texture
        .as_ref()
        .is_some_and(|texture| texture.width() == extent.width && texture.height() == extent.height)
    {
        return;
    }
    *texture = Some(renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ZUI retained pixels"),
        size: wgpu::Extent3d {
            width: extent.width,
            height: extent.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: renderer.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_types::{Color, Rect};

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn presentation_copies_retained_pixels_to_fresh_targets_without_readback() {
        let mut renderer = pollster::block_on(GpuRenderer::new(Default::default())).unwrap();
        #[cfg(feature = "profiling")]
        renderer.set_gpu_timing_enabled(true).unwrap();
        let frame = Frame {
            extent: Extent::new(256, 128),
            scale: 1.0,
            background: Color::BLACK,
        };
        for color in [
            Color::WHITE,
            Color::rgb(200, 80, 40),
            Color::rgb(200, 80, 40),
        ] {
            let mut scene = Scene::new();
            scene
                .painter()
                .fill_rect(Rect::new(10.0, 10.0, 20.0, 20.0), color);
            let expected = renderer.render(&scene, frame).unwrap();
            let mut output = None;
            ensure_texture(&mut output, &renderer, frame.extent);
            let output = output.unwrap();
            renderer
                .render_to_view(&scene, frame, &output.create_view(&Default::default()))
                .unwrap();
            #[cfg(feature = "profiling")]
            {
                assert!(!renderer.take_gpu_time().unwrap().is_zero());
                assert!(renderer.take_gpu_time().unwrap().is_zero());
            }
            let encoder = renderer.device.create_command_encoder(&Default::default());
            let actual = readback::copy(
                &renderer.device,
                &renderer.queue,
                encoder,
                &output,
                frame.extent,
                renderer.limits,
            )
            .unwrap();
            assert_eq!(expected.pixels(), actual.pixels());
        }
        assert!(renderer.damage_stats().reused_frame);
        let mut invalid = Scene::new();
        invalid
            .painter()
            .fill_rect(Rect::new(f64::NAN, 0.0, 1.0, 1.0), Color::WHITE);
        assert!(renderer.render_retained(&invalid, frame).is_err());
        assert!(!renderer.damage_stats().reused_frame);
        renderer.render_retained(&Scene::new(), frame).unwrap();
        assert_eq!(renderer.damage_stats().rasterized_pixels, 256 * 128);
    }
}
