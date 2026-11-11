use crate::{GpuRenderer, backend};
use std::{cell::Cell, sync::mpsc, time::Duration};
use zui_paint::RenderError;

const SLOTS: u32 = 64;
const STRIDE: u64 = wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;

pub(super) struct GpuTiming {
    marker: wgpu::RenderPipeline,
    queries: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    mapped: wgpu::Buffer,
    count: Cell<u32>,
}

impl GpuTiming {
    pub(super) fn begin(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        assert!(
            self.count.get() < SLOTS,
            "collect GPU timing after each frame"
        );
        self.mark(encoder, view, true);
    }

    pub(super) fn end(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let slot = self.count.get();
        self.mark(encoder, view, false);
        self.count.set(slot + 1);
    }

    fn mark(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, start: bool) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ZUI GPU timing marker"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: start.then_some(self.count.get() * 2),
                end_of_pass_write_index: (!start).then_some(self.count.get() * 2 + 1),
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.marker);
        pass.draw(0..3, 0..1);
    }
}

impl GpuRenderer {
    pub fn set_gpu_timing_enabled(&mut self, enabled: bool) -> Result<(), RenderError> {
        self.timing = None;
        if !enabled {
            return Ok(());
        }
        let features = wgpu::Features::TIMESTAMP_QUERY;
        if !self.device.features().contains(features) {
            return Err(backend("GPU timestamps are unavailable"));
        }
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ZUI timestamp marker"),
            source: wgpu::ShaderSource::Wgsl("@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> { let positions = array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0)); return vec4(positions[index],0.0,1.0); } @fragment fn fragment() -> @location(0) vec4<f32> { return vec4(0.0); }".into()),
        });
        self.timing = Some(GpuTiming {
            marker: self
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("ZUI timestamp marker"),
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
                            format: self.target_format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::empty(),
                        })],
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                }),
            queries: self.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("ZUI frame timing"),
                ty: wgpu::QueryType::Timestamp,
                count: SLOTS * 2,
            }),
            resolved: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ZUI resolved timing"),
                size: u64::from(SLOTS) * STRIDE,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            mapped: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ZUI timing readback"),
                size: u64::from(SLOTS) * STRIDE,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            count: Cell::new(0),
        });
        Ok(())
    }

    pub fn take_gpu_time(&mut self) -> Result<Duration, RenderError> {
        let timing = self
            .timing
            .as_ref()
            .ok_or_else(|| backend("GPU timing is disabled"))?;
        let count = timing.count.get();
        if count == 0 {
            return Ok(Duration::ZERO);
        }
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(backend)?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for slot in 0..count {
            encoder.resolve_query_set(
                &timing.queries,
                slot * 2..slot * 2 + 2,
                &timing.resolved,
                u64::from(slot) * STRIDE,
            );
            encoder.copy_buffer_to_buffer(
                &timing.resolved,
                u64::from(slot) * STRIDE,
                &timing.mapped,
                u64::from(slot) * STRIDE,
                16,
            );
        }
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::sync_channel(1);
        timing
            .mapped
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(backend)?;
        receiver.recv().map_err(backend)?.map_err(backend)?;
        let ticks = {
            let bytes = timing
                .mapped
                .slice(..)
                .get_mapped_range()
                .map_err(backend)?;
            (0..count)
                .map(|slot| {
                    let offset = slot as usize * STRIDE as usize;
                    let start = u64::from_ne_bytes(bytes[offset..offset + 8].try_into().unwrap());
                    let end =
                        u64::from_ne_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
                    end.checked_sub(start)
                        .filter(|ticks| *ticks > 0)
                        .ok_or_else(|| backend("invalid GPU timestamp interval"))
                })
                .sum::<Result<u64, RenderError>>()
        };
        timing.mapped.unmap();
        timing.count.set(0);
        Ok(Duration::from_secs_f64(
            ticks? as f64 * f64::from(self.queue.get_timestamp_period()) / 1e9,
        ))
    }
}
