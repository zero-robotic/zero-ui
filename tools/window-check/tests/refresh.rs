use std::time::Duration;
use zui_api::{layout::Dimension, view::View};
use zui_app::{Application, RefreshPhase, ViewAction, ViewApplication, WidgetHost};
use zui_platform::{InputEvent, WindowAction, WindowEvent, WindowMetrics};
use zui_text::ParleyText;
use zui_types::{Extent, Point};
use zui_widgets::column;

struct Model {
    wakes: usize,
    inputs: usize,
    exit_at: usize,
}
impl ViewApplication for Model {
    type Message = ();
    fn view(&self) -> View {
        column([])
            .style(|style| {
                style.width = Dimension::Length(100.0 + self.wakes as f32);
                style.height = Dimension::Length(100.0);
            })
            .build()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.wakes += 1;
            ViewAction {
                rebuild: true,
                window: WindowAction {
                    exit: self.wakes == self.exit_at,
                    redraw: true,
                    wake_at: Some(Duration::from_secs(100 - self.wakes as u64)),
                },
            }
        } else {
            if matches!(event, WindowEvent::Input(_)) {
                self.inputs += 1;
            }
            ViewAction::default()
        }
    }
}
fn metrics() -> WindowMetrics {
    WindowMetrics {
        extent: Extent::new(800, 480),
        scale: 1.0,
        occluded: false,
    }
}
fn host(exit_at: usize) -> WidgetHost<Model, zui_layout::TaffyLayout, ParleyText> {
    WidgetHost::new(
        Model {
            wakes: 0,
            inputs: 0,
            exit_at,
        },
        Default::default(),
        ParleyText::default(),
        Default::default(),
    )
    .unwrap()
}
#[test]
fn wakes_preserve_model_updates_merge_deadlines_and_stop_on_exit() {
    let mut host = host(3);
    host.set_profiling(true);
    let result = host
        .update_batch(vec![WindowEvent::Wake; 10], metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().wakes, 3);
    assert!(result.exit && result.redraw);
    assert_eq!(result.wake_at, Some(Duration::from_secs(97)));
    assert_eq!(
        host.refresh_metrics().phase(RefreshPhase::Animation).calls,
        1
    );
    assert_eq!(host.refresh_metrics().rebuild_requests, 3);
    assert_eq!(host.refresh_metrics().coalesced_rebuilds, 2);
}
#[test]
fn input_barriers_commit_geometry_and_preserve_every_input() {
    let mut host = host(100);
    host.set_profiling(true);
    let input = WindowEvent::Input(InputEvent::PointerMoved(Point::new(400.0, 400.0)));
    host.update_batch(
        [WindowEvent::Wake, WindowEvent::Wake, input.clone(), input],
        metrics(),
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(host.model().inputs, 2);
    assert_eq!(
        host.tree()
            .layout_box(host.tree().root())
            .unwrap()
            .bounds
            .size
            .width,
        102.0
    );
    assert_eq!(host.refresh_metrics().phase(RefreshPhase::View).calls, 1);
    assert_eq!(host.refresh_metrics().phase(RefreshPhase::Input).calls, 2);
    assert_eq!(
        host.refresh_metrics().phase(RefreshPhase::Animation).calls,
        3
    );
    let before = host.refresh_metrics().phase(RefreshPhase::Animation).calls;
    host.update_batch([], metrics(), Duration::ZERO).unwrap();
    assert_eq!(
        host.refresh_metrics().phase(RefreshPhase::Animation).calls,
        before
    );
}
#[test]
fn profiling_is_opt_in_and_reset_preserves_enabled_state() {
    let mut host = host(100);
    host.update(WindowEvent::Wake, metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.refresh_metrics().rebuild_requests, 0);
    host.set_profiling(true);
    host.update(WindowEvent::Wake, metrics(), Duration::ZERO)
        .unwrap();
    host.record_presentation(Duration::from_millis(2));
    let stats = host.take_refresh_metrics();
    assert_eq!(
        stats.phase(RefreshPhase::Present).total,
        Duration::from_millis(2)
    );
    assert_eq!(stats.rebuild_requests, 1);
    assert!(host.profiling_enabled());
    assert_eq!(host.refresh_metrics().rebuild_requests, 0);
}
