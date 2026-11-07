//! Synthetic telemetry and scheduling live in the application, not chart widgets.
use std::{error::Error, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_app::{Backend, ViewAction, ViewApplication, WidgetHost};
use zui_charts::{ChartRange, SparklineHistory, SparklineSeries, SparklineView};
use zui_dashboard::{DashboardPanelView, MetricCardView, MetricChange, MetricTone};
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
    ) -> Result<Self, zui_charts::HistoryError> {
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
                View::new(ProgressBarView::themed(
                    &theme,
                    ProgressValue::Determinate(self.value / 100.0),
                )),
                View::new(TextView::themed(
                    &theme,
                    format!(
                        "负载偏移 {:+.0}% · 拖动滑块同步指标、进度条与趋势；暂停后仍可手动调整。",
                        self.bias
                    ),
                    self.families.clone(),
                    TypeRole::Body,
                )),
                View::new(SliderView::themed(
                    &theme,
                    NumericRange::new(-20.0, 20.0, 1.0).unwrap(),
                    self.bias,
                    ActionId(2),
                ))
                .keyed("bias")
                .styled(LayoutStyle {
                    height: Dimension::Length(36.0),
                    ..Default::default()
                }),
                View::new(SwitchView::themed(&theme, self.running, ActionId(3))).keyed("running"),
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
mod tests {
    use super::*;
    use zui_api::action::WidgetAction;
    #[test]
    fn pause_adjust_resume_updates_history_and_all_dashboard_components() {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut model = Demo::new(families, None).unwrap();
        let metrics = WindowMetrics {
            extent: zui_types::Extent::new(1100, 800),
            scale: 1.0,
            occluded: false,
        };
        model.update(WindowEvent::Resumed, metrics, Duration::ZERO);
        let mut tree = zui_core::Tree::from_view(model.view(), Default::default()).unwrap();
        for action in [
            WidgetAction::new(ActionId(3), SwitchChanged { checked: false }),
            WidgetAction::new(ActionId(2), SliderChanged { value: 20.0 }),
        ] {
            let message = model
                .map_action(&ActionEvent {
                    source: tree.root(),
                    action,
                })
                .unwrap();
            assert!(model.message(message).rebuild);
            tree.reconcile(model.view()).unwrap();
            for size in [Size::new(1100.0, 800.0), Size::new(480.0, 640.0)] {
                tree.layout_with_text(&mut zui_layout::TaffyLayout::default(), &mut text, size)
                    .unwrap();
                tree.scene().unwrap().validate(Default::default()).unwrap();
            }
        }
        assert!(!model.running);
        assert_eq!(model.next, None);
        assert_eq!(model.value, sample(15) + 20.0);
        assert_eq!(model.history.len(), 17);
        model.update(WindowEvent::Wake, metrics, Duration::from_secs(1));
        assert_eq!(model.history.len(), 17);
        let message = model
            .map_action(&ActionEvent {
                source: tree.root(),
                action: WidgetAction::new(ActionId(1), ButtonActivated),
            })
            .unwrap();
        model.message(message);
        assert!(model.running);
        model.update(WindowEvent::Wake, metrics, Duration::from_secs(1));
        model.update(WindowEvent::Wake, metrics, Duration::from_secs(1) + PERIOD);
        assert_eq!(model.history.len(), 18);
        assert_eq!(model.value, sample(16) + 20.0);
    }
}
