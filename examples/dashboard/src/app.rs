//! Synthetic telemetry and scheduling live in the application, not chart widgets.
mod views;
use std::{error::Error, time::Duration};
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
use zui_widgets::{
    ButtonActivated, ButtonView, NumericRange, ProgressBarView, ProgressValue, ScrollView,
    SliderChanged, SliderView, SwitchChanged, SwitchView, TextView, column, row,
};
const PERIOD: Duration = Duration::from_millis(250);
const CAPACITY: usize = 120;
pub fn run(
    config: WindowConfig,
    backend: Backend,
    smoke: Option<Duration>,
) -> Result<RunReport, Box<dyn Error>> {
    let mut text = zui_text::ParleyText::with_system_fonts(Default::default());
    let families = text.resolve_families(&[])?;
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
    bias: f64,
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
            bias: 0.0,
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
enum Message {
    Toggle,
    Running(bool),
    Bias(f64),
}
impl ViewApplication for Demo {
    type Message = Message;
    fn view(&self) -> View {
        self.dashboard_view()
    }
    fn map_action(&self, event: &ActionEvent) -> Option<Message> {
        match event.action.id {
            ActionId(1) if event.action.payload::<ButtonActivated>().is_some() => {
                Some(Message::Toggle)
            }
            ActionId(2) => event
                .action
                .payload::<SliderChanged>()
                .map(|e| Message::Bias(e.value)),
            ActionId(3) => event
                .action
                .payload::<SwitchChanged>()
                .map(|e| Message::Running(e.checked)),
            _ => None,
        }
    }
    fn message(&mut self, message: Message) -> ViewAction {
        match message {
            Message::Toggle => self.running = !self.running,
            Message::Running(value) => self.running = value,
            Message::Bias(value) => {
                self.bias = value;
                self.value = (sample(self.ticks.saturating_sub(1)) + self.bias).clamp(0.0, 100.0);
                self.history.push(Some(self.value)).unwrap();
                self.series = self.history.snapshot();
                return self.response(true);
            }
        }
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
                self.value = (sample(self.ticks) + self.bias).clamp(0.0, 100.0);
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
mod tests;
