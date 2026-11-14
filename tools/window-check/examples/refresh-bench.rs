//! Deterministic headless refresh benchmark. Release build; no window/system font I/O.
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use zui_api::text::{TextError, TextLayout, TextRequest, TextService};
use zui_app::{Application, RefreshPhase, WidgetHost};
use zui_platform::WindowMetrics;
use zui_types::{Color, Extent};
mod support;
use support::{Model, events, percentile};
struct TextCounter {
    inner: zui_text::ParleyText,
    calls: Rc<Cell<u64>>,
}
impl TextService for TextCounter {
    fn font_epoch(&self) -> u64 {
        self.inner.font_epoch()
    }
    fn layout(&mut self, r: TextRequest<'_>) -> Result<TextLayout, TextError> {
        self.calls.set(self.calls.get() + 1);
        self.inner.layout(r)
    }
}
#[cfg(feature = "cpu")]
fn main() {
    let samples: usize = std::env::args()
        .nth(1)
        .map(|s| s.parse().unwrap())
        .unwrap_or(100);
    assert!(samples > 0);
    println!(
        "scenario,samples,p50_us,p95_us,update_us,draw_us,cpu_us,view_us,reconcile_us,layout_us,input_us,animation_us,scene_us,view_calls,layout_passes,animation_calls,shape_calls,coalesced,checksum,last_rasterized_pixels,total_pixels"
    );
    for kind in [
        "local_text",
        "color",
        "popup",
        "burst32",
        "pointer32",
        "virtual_idle",
    ] {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let calls = Rc::new(Cell::new(0));
        let mut host = WidgetHost::new(
            Model {
                kind,
                value: 0,
                families,
            },
            zui_layout::TaffyLayout::default(),
            TextCounter {
                inner: text,
                calls: calls.clone(),
            },
            Default::default(),
        )
        .unwrap();
        host.set_profiling(true);
        let metrics = WindowMetrics {
            extent: Extent::new(800, 480),
            scale: 1.0,
            occluded: false,
        };
        let frame = zui_paint::Frame {
            extent: metrics.extent,
            scale: 1.0,
            background: Color::BLACK,
        };
        let mut renderer = zui_render_cpu::RetainedCpuRenderer::default();
        let mut scene = zui_paint::Scene::new();
        host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
        let mut totals = Vec::new();
        let (mut update, mut draw, mut cpu) = (0.0, 0.0, 0.0);
        let mut checksum = 0u64;
        for iteration in 0..samples + 20 {
            if iteration == 20 {
                host.take_refresh_metrics();
                calls.set(0);
            }
            let elapsed = Duration::from_millis(iteration as u64 * 16);
            let events = events(kind);
            let begin = Instant::now();
            host.update_batch(events, metrics, elapsed).unwrap();
            let after_update = Instant::now();
            host.draw(&mut scene, metrics, elapsed).unwrap();
            let after_draw = Instant::now();
            let pixels = renderer.render(&scene, frame).unwrap();
            let end = Instant::now();
            host.record_presentation(end - after_draw);
            if iteration >= 20 {
                totals.push((end - begin).as_secs_f64() * 1e6);
                update += (after_update - begin).as_secs_f64() * 1e6;
                draw += (after_draw - after_update).as_secs_f64() * 1e6;
                cpu += (end - after_draw).as_secs_f64() * 1e6;
                // Outside the timed interval; verify every frame, not only the last image.
                let hash = pixels.pixels().iter().fold(0xcbf29ce484222325u64, |h, &v| {
                    (h ^ v as u64).wrapping_mul(0x100000001b3)
                });
                checksum = checksum.rotate_left(7) ^ hash;
            }
        }
        let stats = host.refresh_metrics();
        let n = samples as f64;
        let stage = |p| stats.phase(p).total.as_secs_f64() * 1e6 / n;
        println!(
            "{kind},{samples},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{},{},{checksum:016x},{},{}",
            percentile(&mut totals, 0.5),
            percentile(&mut totals, 0.95),
            update / n,
            draw / n,
            cpu / n,
            stage(RefreshPhase::View),
            stage(RefreshPhase::Reconcile),
            stage(RefreshPhase::Layout),
            stage(RefreshPhase::Input),
            stage(RefreshPhase::Animation),
            stage(RefreshPhase::Scene),
            stats.phase(RefreshPhase::View).calls,
            stats.layouts,
            stats.phase(RefreshPhase::Animation).calls,
            calls.get(),
            stats.coalesced_rebuilds,
            renderer.damage_stats().rasterized_pixels,
            renderer.damage_stats().total_pixels
        );
    }
}
