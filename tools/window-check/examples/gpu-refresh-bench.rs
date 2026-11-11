#[allow(dead_code, unused_imports)]
#[path = "refresh-bench.rs"]
mod workload;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use zui_app::{Application, WidgetHost};
use zui_paint::{DamageConfig, Frame, Scene};
use zui_platform::{InputEvent, WindowEvent, WindowMetrics};
use zui_render_gpu::GpuRenderer;
use zui_types::{Color, Extent, Point};

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction) as usize]
}

fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    let mode = arguments.get(1).map(String::as_str).unwrap_or("retained");
    assert!(matches!(mode, "full" | "retained"));
    let samples: usize = arguments
        .get(2)
        .map(|value| value.parse().unwrap())
        .unwrap_or(120);
    let scale: f64 = arguments
        .get(3)
        .map(|value| value.parse().unwrap())
        .unwrap_or(1.0);
    assert!(samples > 0 && scale > 0.0);
    println!(
        "mode,scenario,scale,samples,gpu_p50_us,gpu_p95_us,frame_p50_us,frame_p95_us,submit_p50_us,submit_p95_us,last_rasterized_pixels"
    );
    for kind in [
        "local_text",
        "color",
        "popup",
        "burst32",
        "pointer32",
        "virtual_idle",
        "full_damage",
    ] {
        let mut gpu_times = Vec::new();
        let mut frame_times = Vec::new();
        let mut submit_times = Vec::new();
        let mut rasterized = 0;
        for timestamps in [false, true] {
            let mut text = zui_text::ParleyText::default();
            let families = text
                .register_font(Arc::from(
                    include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
                ))
                .unwrap();
            let mut host = WidgetHost::new(
                workload::Model {
                    kind,
                    value: 0,
                    families,
                },
                zui_layout::TaffyLayout::default(),
                text,
                Default::default(),
            )
            .unwrap();
            let metrics = WindowMetrics {
                extent: Extent::new((800.0 * scale) as u32, (480.0 * scale) as u32),
                scale,
                occluded: false,
            };
            let frame = Frame {
                extent: metrics.extent,
                scale,
                background: Color::BLACK,
            };
            let mut renderer = pollster::block_on(GpuRenderer::new(Default::default())).unwrap();
            if kind == "local_text" && !timestamps {
                eprintln!(
                    "adapter={}, mode={mode}, scale={scale}",
                    renderer.adapter_name()
                );
            }
            if mode == "full" {
                renderer.set_damage_config(DamageConfig {
                    max_pixel_bytes: 0,
                    ..Default::default()
                });
            }
            renderer.set_gpu_timing_enabled(timestamps).unwrap();
            let target = renderer.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("GPU benchmark presentation target"),
                size: wgpu::Extent3d {
                    width: frame.extent.width,
                    height: frame.extent.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: renderer.target_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = target.create_view(&Default::default());
            let mut scene = Scene::new();
            host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
            for iteration in 0..samples + 30 {
                let elapsed = Duration::from_millis(iteration as u64 * 16);
                let events = if kind == "pointer32" {
                    (0..32)
                        .map(|index| {
                            WindowEvent::Input(InputEvent::PointerMoved(Point::new(
                                index as f64 * 5.0,
                                470.0,
                            )))
                        })
                        .collect()
                } else {
                    vec![WindowEvent::Wake; if kind == "burst32" { 32 } else { 1 }]
                };
                let begin = Instant::now();
                host.update_batch(events, metrics, elapsed).unwrap();
                host.draw(&mut scene, metrics, elapsed).unwrap();
                renderer.render_to_view(&scene, frame, &view).unwrap();
                let submitted = Instant::now();
                renderer
                    .device()
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                let complete = Instant::now();
                let gpu_time = if timestamps {
                    renderer.take_gpu_time().unwrap()
                } else {
                    Duration::ZERO
                };
                if iteration >= 30 {
                    if timestamps {
                        gpu_times.push(gpu_time.as_secs_f64() * 1e6);
                    } else {
                        frame_times.push((complete - begin).as_secs_f64() * 1e6);
                        submit_times.push((submitted - begin).as_secs_f64() * 1e6);
                    }
                }
            }
            rasterized = if mode == "full" {
                u64::from(frame.extent.width) * u64::from(frame.extent.height)
            } else {
                renderer.damage_stats().rasterized_pixels
            };
        }
        println!(
            "{mode},{kind},{scale},{samples},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{rasterized}",
            percentile(&mut gpu_times, 0.5),
            percentile(&mut gpu_times, 0.95),
            percentile(&mut frame_times, 0.5),
            percentile(&mut frame_times, 0.95),
            percentile(&mut submit_times, 0.5),
            percentile(&mut submit_times, 0.95)
        );
    }
}
