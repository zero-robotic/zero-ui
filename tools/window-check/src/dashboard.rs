//! Synthetic telemetry and scheduling live in the application, not chart widgets.
use std::{error::Error, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_app::{Backend, ViewAction, ViewApplication, WidgetHost};
use zui_extras::charts::{ChartRange, SparklineHistory, SparklineSeries, SparklineView};
use zui_extras::dashboard::{DashboardPanelView, MetricCardView, MetricChange, MetricTone};
use zui_platform::{
    InputEvent, Key, RunReport, WindowAction, WindowConfig, WindowEvent, WindowMetrics,
};
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{ButtonActivated, ButtonView, ScrollView, TextView, column, row};
const PERIOD: Duration = Duration::from_millis(250);
const CAPACITY: usize = 120;
pub fn run(
    config: WindowConfig,
    backend: Backend,
    smoke: Option<Duration>,
) -> Result<RunReport, Box<dyn Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let model = Demo::new(families, smoke)?;
    let host = WidgetHost::new(
        model,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?;
    Ok(zui_app::run(config, backend, host)?)
}
struct Demo {
    families: Vec<String>,
    history: SparklineHistory,
    series: SparklineSeries,
    running: bool,
    occluded: bool,
    ticks: u64,
    value: f64,
    next: Option<Duration>,
    smoke: Option<Duration>,
    deadline: Option<Duration>,
}
impl Demo {
    fn new(
        families: Vec<String>,
        smoke: Option<Duration>,
    ) -> Result<Self, zui_extras::charts::HistoryError> {
        let mut history = SparklineHistory::new(CAPACITY)?;
        for index in 0..16 {
            history.push(Some(sample(index)))?;
        }
        let series = history.snapshot();
        Ok(Self {
            families,
            history,
            series,
            running: true,
            occluded: false,
            ticks: 16,
            value: sample(15),
            next: None,
            smoke,
            deadline: None,
        })
    }
    fn response(&self, rebuild: bool) -> ViewAction {
        ViewAction {
            rebuild,
            window: WindowAction {
                redraw: rebuild,
                wake_at: self.next.into_iter().chain(self.deadline).min(),
                ..Default::default()
            },
        }
    }
}
fn sample(tick: u64) -> f64 {
    let phase = tick % 40;
    20.0 + 3.0
        * if phase <= 20 {
            phase as f64
        } else {
            (40 - phase) as f64
        }
}
impl ViewApplication for Demo {
    type Message = ();
    fn view(&self) -> View {
        let theme = Theme::dashboard();
        let mut load = MetricCardView::themed(
            &theme,
            "模拟设备负载",
            format!("{:.0}", self.value),
            self.families.clone(),
        );
        load.unit = Some("%".into());
        load.change = Some(MetricChange::new(
            if self.running {
                "采样中 · 250 ms"
            } else {
                "已暂停"
            },
            MetricTone::Neutral,
        ));
        load.layout.width = Dimension::Length(220.0);
        load.key = Some("load".into());
        let mut count = MetricCardView::themed(
            &theme,
            "保留样本槽位",
            self.history.len().to_string(),
            self.families.clone(),
        );
        count.unit = Some(format!("/ {CAPACITY}"));
        count.change = Some(MetricChange::new(
            "有界历史 · 缺测保留断线",
            MetricTone::Neutral,
        ));
        count.layout.width = Dimension::Length(220.0);
        count.key = Some("history".into());
        let mut trend = SparklineView::themed(&theme, self.series.clone());
        trend.range = Some(ChartRange::new(0.0, 100.0).expect("fixed increasing range"));
        trend.style.natural_size.height = 96.0;
        let panel = DashboardPanelView::themed(
            &theme,
            [
                View::new(TextView::themed(
                    &theme,
                    "ZUI · 实时监测演示（模拟数据）",
                    self.families.clone(),
                    TypeRole::Heading,
                ))
                .keyed("heading"),
                row([load.build(), count.build()])
                    .keyed("metrics")
                    .style(|s| {
                        s.wrap = true;
                        s.gap = Size::new(16.0, 16.0);
                    })
                    .build(),
                View::new(trend).keyed("trend"),
                View::new(ButtonView::themed(
                    &theme,
                    if self.running {
                        "暂停采样"
                    } else {
                        "恢复采样"
                    },
                    self.families.clone(),
                    ActionId(1),
                ))
                .keyed("pause")
                .styled(LayoutStyle {
                    width: Dimension::Length(140.0),
                    padding: Edges::all(10.0),
                    ..Default::default()
                }),
            ],
        )
        .build();
        ScrollView::vertical(
            column([panel])
                .style(|s| s.padding = Edges::all(20.0))
                .build(),
        )
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
        })
        .build()
    }
    fn map_action(&self, event: &ActionEvent) -> Option<()> {
        (event.action.id == ActionId(1) && event.action.payload::<ButtonActivated>().is_some())
            .then_some(())
    }
    fn message(&mut self, _: ()) -> ViewAction {
        self.running = !self.running;
        // A fresh wake obtains current elapsed time; no stale timestamp from a
        // previous input callback and no replay of samples accumulated while paused.
        self.next = (self.running && !self.occluded).then_some(Duration::ZERO);
        self.response(true)
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, elapsed: Duration) -> ViewAction {
        if self.deadline.is_some_and(|deadline| elapsed >= deadline) {
            return ViewAction::exit();
        }
        match event {
            WindowEvent::CloseRequested => ViewAction::exit(),
            WindowEvent::Input(InputEvent::Key(key)) if key.pressed && key.key == Key::Escape => {
                ViewAction::exit()
            }
            WindowEvent::Occluded(true) => {
                self.occluded = true;
                self.next = None;
                self.response(false)
            }
            WindowEvent::Occluded(false) => {
                self.occluded = false;
                self.next = if self.running {
                    elapsed.checked_add(PERIOD)
                } else {
                    None
                };
                self.response(false)
            }
            WindowEvent::Resumed => {
                self.occluded = false;
                if self.deadline.is_none() {
                    self.deadline = self
                        .smoke
                        .and_then(|duration| elapsed.checked_add(duration));
                }
                self.next = if self.running {
                    elapsed.checked_add(PERIOD)
                } else {
                    None
                };
                self.response(false)
            }
            WindowEvent::Wake if self.running && !self.occluded => {
                if self.next == Some(Duration::ZERO) || self.next.is_none() {
                    self.next = elapsed.checked_add(PERIOD);
                    return self.response(false);
                }
                let due = self.next.expect("checked above");
                if elapsed < due {
                    return self.response(false);
                }
                // Bound late-event work. Missing nominal slots are explicit gaps;
                // no synthetic historical values or unbounded catch-up loop.
                let missed = (elapsed.saturating_sub(due).as_millis() / PERIOD.as_millis())
                    .min(CAPACITY as u128) as usize;
                for _ in 0..missed {
                    self.history.push(None).expect("missing sample is valid");
                }
                self.value = sample(self.ticks);
                self.ticks = self.ticks.saturating_add(1);
                self.history
                    .push(Some(self.value))
                    .expect("synthetic value is finite");
                self.series = self.history.snapshot();
                self.next = elapsed.checked_add(PERIOD);
                self.response(true)
            }
            _ => self.response(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_app::Application;
    use zui_paint::Scene;
    use zui_platform::KeyInput;
    use zui_types::Extent;
    fn metrics() -> WindowMetrics {
        WindowMetrics {
            extent: Extent::new(800, 480),
            scale: 1.0,
            occluded: false,
        }
    }
    #[test]
    fn sampling_deadlines_pause_resume_occlusion_and_late_events_are_bounded() {
        let mut model = Demo::new(Vec::new(), None).unwrap();
        let metrics = metrics();
        let action = model.update(WindowEvent::Resumed, metrics, Duration::ZERO);
        assert_eq!(action.window.wake_at, Some(PERIOD));
        assert!(
            !model
                .update(WindowEvent::Wake, metrics, Duration::from_millis(249))
                .rebuild
        );
        assert_eq!(model.ticks, 16);
        assert!(model.update(WindowEvent::Wake, metrics, PERIOD).rebuild);
        assert_eq!(model.ticks, 17);
        let late = Duration::from_secs(3600);
        let action = model.update(WindowEvent::Wake, metrics, late);
        assert_eq!(action.window.wake_at, Some(late + PERIOD));
        assert_eq!(model.history.len(), CAPACITY);
        assert_eq!(
            model.history.iter().filter(Option::is_none).count(),
            CAPACITY - 1
        );
        assert_eq!(model.ticks, 18);
        let old = model.series.clone();
        assert!(model.message(()).window.wake_at.is_none());
        assert!(
            !model
                .update(WindowEvent::Wake, metrics, late + PERIOD)
                .rebuild
        );
        assert_eq!(model.series, old);
        assert_eq!(model.message(()).window.wake_at, Some(Duration::ZERO));
        let now = late + Duration::from_secs(30);
        assert_eq!(
            model.update(WindowEvent::Wake, metrics, now).window.wake_at,
            Some(now + PERIOD)
        );
        assert_eq!(model.ticks, 18);
        model.update(WindowEvent::Occluded(true), metrics, now);
        assert!(
            model
                .update(WindowEvent::Wake, metrics, now + PERIOD)
                .window
                .wake_at
                .is_none()
        );
        assert_eq!(model.ticks, 18);
        assert_eq!(
            model
                .update(WindowEvent::Resumed, metrics, now + PERIOD)
                .window
                .wake_at,
            Some(now + PERIOD * 2)
        );
    }
    #[test]
    fn smoke_deadline_remains_active_while_sampling_is_paused() {
        let mut model = Demo::new(Vec::new(), Some(Duration::from_secs(1))).unwrap();
        model.update(WindowEvent::Resumed, metrics(), Duration::ZERO);
        assert_eq!(
            model.message(()).window.wake_at,
            Some(Duration::from_secs(1))
        );
        assert!(
            model
                .update(WindowEvent::Wake, metrics(), Duration::from_secs(1))
                .window
                .exit
        );
    }
    #[test]
    fn host_button_round_trip_keeps_focus_and_resized_dashboard_lays_out() {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut host = WidgetHost::new(
            Demo::new(families, None).unwrap(),
            zui_layout::TaffyLayout::default(),
            text,
            Default::default(),
        )
        .unwrap();
        let metrics = metrics();
        host.update(WindowEvent::Resumed, metrics, Duration::ZERO)
            .unwrap();
        let key = |key| {
            WindowEvent::Input(InputEvent::Key(KeyInput {
                key,
                pressed: true,
                repeat: false,
                text: None,
                modifiers: Default::default(),
            }))
        };
        // The scroll container is the first focus stop; the pause button is next.
        host.update(key(Key::Tab), metrics, Duration::ZERO).unwrap();
        host.update(key(Key::Tab), metrics, Duration::ZERO).unwrap();
        let button = host.tree().focused();
        assert!(button.is_some());
        host.update(key(Key::Enter), metrics, Duration::ZERO)
            .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert!(!host.model().running);
        assert_eq!(host.tree().focused(), button);
        let ticks = host.model().ticks;
        host.update(WindowEvent::Wake, metrics, Duration::from_secs(1))
            .unwrap();
        assert_eq!(host.model().ticks, ticks);
        host.update(key(Key::Enter), metrics, Duration::from_secs(1))
            .unwrap();
        host.update(WindowEvent::Wake, metrics, Duration::from_secs(1))
            .unwrap();
        host.update(WindowEvent::Wake, metrics, Duration::from_millis(1250))
            .unwrap();
        assert_eq!(host.model().ticks, ticks + 1);
        let narrow = WindowMetrics {
            extent: Extent::new(320, 260),
            ..metrics
        };
        host.draw(&mut Scene::new(), narrow, Duration::from_millis(1250))
            .unwrap();
        assert_eq!(host.tree().focused(), button);
    }
}
