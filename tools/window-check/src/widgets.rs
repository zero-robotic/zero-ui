use std::{error::Error, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges},
    view::View,
};
use zui_app::{Backend, ViewAction, ViewApplication, WidgetHost};
use zui_platform::{
    InputEvent, Key, RunReport, WindowAction, WindowConfig, WindowEvent, WindowMetrics,
};
use zui_theme::{ColorRole, Space, Theme, TypeRole};
use zui_types::{Color, Size};
use zui_widgets::{
    ButtonActivated, ButtonView, CheckBoxView, CheckChanged, CheckState, Container, NumericRange,
    ProgressBarView, ProgressValue, RadioChanged, RadioGroupView, RadioOption, RadioOptionId,
    SliderChanged, SliderView, StepperChanged, StepperView, TextInputEvent, TextInputView,
    TextView, column, row,
};

pub fn run(
    config: WindowConfig,
    backend: Backend,
    smoke: Option<Duration>,
) -> Result<RunReport, Box<dyn Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let model = Demo {
        families,
        light: false,
        details: CheckState::Checked,
        mode: RadioOptionId(1),
        level: 50.0,
        input: "ZUI 中文输入".into(),
        smoke,
        deadline: None,
        switch_at: None,
    };
    let host = WidgetHost::new(
        model,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?
    .with_clipboard(zui_app::clipboard::NativeClipboard::default());
    Ok(zui_app::run(config, backend, host)?)
}
struct Demo {
    families: Vec<String>,
    light: bool,
    details: CheckState,
    mode: RadioOptionId,
    level: f64,
    input: String,
    smoke: Option<Duration>,
    deadline: Option<Duration>,
    switch_at: Option<Duration>,
}
enum Message {
    ToggleTheme,
    SetDetails(CheckState),
    SetMode(RadioOptionId),
    SetLevel(f64),
    SetInput(String),
}
impl Demo {
    fn theme(&self) -> Theme {
        if self.light {
            Theme::light()
        } else {
            Theme::dashboard()
        }
    }
}
impl ViewApplication for Demo {
    type Message = Message;
    fn map_action(&self, event: &ActionEvent) -> Option<Message> {
        match event.action.id {
            ActionId(1) if event.action.payload::<ButtonActivated>().is_some() => {
                Some(Message::ToggleTheme)
            }
            ActionId(2) => event
                .action
                .payload::<CheckChanged>()
                .map(|change| Message::SetDetails(change.state)),
            ActionId(3) => event
                .action
                .payload::<RadioChanged>()
                .map(|change| Message::SetMode(change.selected)),
            ActionId(4) => event
                .action
                .payload::<SliderChanged>()
                .map(|change| Message::SetLevel(change.value)),
            ActionId(6) => event
                .action
                .payload::<StepperChanged>()
                .map(|change| Message::SetLevel(change.value)),
            ActionId(5) => match event.action.payload::<TextInputEvent>() {
                Some(TextInputEvent::Changed(text)) => Some(Message::SetInput(text.clone())),
                _ => None,
            },
            _ => None,
        }
    }
    fn message(&mut self, message: Message) -> ViewAction {
        match message {
            Message::ToggleTheme => self.light = !self.light,
            Message::SetDetails(state) => self.details = state,
            Message::SetMode(mode) => self.mode = mode,
            Message::SetLevel(level) => self.level = level,
            Message::SetInput(text) => self.input = text,
        }
        println!(
            "control message: light={}, details={:?}, mode={:?}, level={}",
            self.light, self.details, self.mode, self.level
        );
        ViewAction::rebuild()
    }
    fn view(&self) -> View {
        let theme = self.theme();
        let mut title = TextView::themed(
            &theme,
            "ZUI · 控件窗口",
            self.families.clone(),
            TypeRole::Heading,
        );
        title.color = theme.color(ColorRole::Accent);
        let body = TextView::themed(
            &theme,
            "点击按钮切换主题，或按 Tab 聚焦后用 Space / Enter 激活。窗口变窄时文字重新换行，Escape 退出。",
            self.families.clone(),
            TypeRole::Body,
        );
        let caption = TextView::themed(
            &theme,
            if self.details == CheckState::Checked {
                if self.mode == RadioOptionId(1) {
                    "运行模式：自动 · View → 布局与排版 → CPU / GPU"
                } else {
                    "运行模式：手动 · View → 布局与排版 → CPU / GPU"
                }
            } else {
                "详细说明已关闭"
            },
            self.families.clone(),
            TypeRole::Caption,
        );
        let mut stepper = StepperView::themed(
            &theme,
            self.families.clone(),
            self.level,
            NumericRange::new(0.0, 100.0, 5.0).expect("constant range"),
            ActionId(6),
        );
        stepper.display = Some(format!("{:.0}%", self.level));
        stepper.key = Some("level-stepper".into());
        column([
            View::new(title).keyed("title"),
            Container::new(
                column([
                    View::new(body).keyed("body"),
                    View::new(caption).keyed("caption"),
                ])
                .style(|s| s.gap = Size::new(0.0, theme.space(Space::Md) as f64))
                .build(),
            )
            .background(theme.color(ColorRole::Surface))
            .keyed("panel")
            .style(|s| s.padding = Edges::all(theme.space(Space::Lg)))
            .build(),
            row([
                column([
                    View::new(ButtonView::themed(
                        &theme,
                        "切换主题",
                        self.families.clone(),
                        ActionId(1),
                    ))
                    .keyed("toggle-theme")
                    .styled(zui_api::layout::LayoutStyle {
                        width: Dimension::Length(200.0),
                        min_height: Dimension::Length(44.0),
                        padding: Edges {
                            left: 16.0,
                            right: 16.0,
                            top: 8.0,
                            bottom: 8.0,
                        },
                        ..Default::default()
                    }),
                    View::new(CheckBoxView::themed(
                        &theme,
                        "显示详细说明",
                        self.families.clone(),
                        self.details,
                        ActionId(2),
                    ))
                    .keyed("show-details")
                    .styled(zui_api::layout::LayoutStyle {
                        min_height: Dimension::Length(40.0),
                        padding: Edges::all(8.0),
                        ..Default::default()
                    }),
                ])
                .style(|s| s.gap = Size::new(0.0, 12.0))
                .build(),
                View::new(RadioGroupView::themed(
                    &theme,
                    vec![
                        RadioOption::new(RadioOptionId(1), "自动模式"),
                        RadioOption::new(RadioOptionId(2), "手动模式"),
                    ],
                    self.families.clone(),
                    Some(self.mode),
                    ActionId(3),
                ))
                .keyed("mode")
                .styled(zui_api::layout::LayoutStyle {
                    width: Dimension::Length(200.0),
                    padding: Edges::all(8.0),
                    ..Default::default()
                }),
            ])
            .style(|s| {
                s.gap = Size::new(24.0, 12.0);
                s.wrap = true;
            })
            .build(),
            row([
                column([
                    View::new(TextView::themed(
                        &theme,
                        format!("设定值：{:.0}%", self.level),
                        self.families.clone(),
                        TypeRole::Caption,
                    )),
                    View::new(SliderView::themed(
                        &theme,
                        NumericRange::new(0.0, 100.0, 5.0).expect("constant range"),
                        self.level,
                        ActionId(4),
                    ))
                    .keyed("level")
                    .styled(zui_api::layout::LayoutStyle {
                        height: Dimension::Length(36.0),
                        padding: Edges::all(8.0),
                        ..Default::default()
                    }),
                    stepper
                        .build()
                        .expect("model level remains within the shared range"),
                    View::new(ProgressBarView::themed(
                        &theme,
                        ProgressValue::Determinate(self.level / 100.0),
                    ))
                    .keyed("level-progress"),
                ])
                .style(|s| {
                    s.gap = Size::new(0.0, 8.0);
                    s.width = Dimension::Length(320.0);
                })
                .build(),
                View::new(TextInputView::themed(
                    &theme,
                    self.input.clone(),
                    self.families.clone(),
                    ActionId(5),
                ))
                .keyed("input")
                .styled(zui_api::layout::LayoutStyle {
                    width: Dimension::Length(320.0),
                    height: Dimension::Length(48.0),
                    padding: Edges::all(8.0),
                    ..Default::default()
                }),
            ])
            .style(|s| {
                s.gap = Size::new(24.0, 12.0);
                s.wrap = true;
            })
            .build(),
        ])
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
            s.padding = Edges::all(theme.space(Space::Lg));
            s.gap = Size::new(0.0, theme.space(Space::Lg) as f64);
        })
        .build()
    }
    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> ViewAction {
        match event {
            WindowEvent::Resumed => {
                println!(
                    "widget window: logical={:?}, scale={}",
                    metrics.logical_size(),
                    metrics.scale
                );
                self.deadline = self.smoke.map(|duration| elapsed + duration);
                self.switch_at = self.smoke.map(|duration| elapsed + duration / 2);
                ViewAction {
                    window: WindowAction {
                        wake_at: self.switch_at,
                        ..Default::default()
                    },
                    rebuild: false,
                }
            }
            WindowEvent::CloseRequested => ViewAction::exit(),
            WindowEvent::Wake if self.deadline.is_some_and(|time| elapsed >= time) => {
                ViewAction::exit()
            }
            WindowEvent::Wake if self.switch_at.is_some_and(|time| elapsed >= time) => {
                self.switch_at = None;
                self.light = !self.light;
                println!("widget smoke: theme switched");
                ViewAction {
                    window: WindowAction {
                        wake_at: self.deadline,
                        ..Default::default()
                    },
                    rebuild: true,
                }
            }
            WindowEvent::Input(InputEvent::Key(input)) if input.pressed && !input.repeat => {
                match input.key {
                    Key::Escape => ViewAction::exit(),
                    _ => ViewAction::default(),
                }
            }
            _ => ViewAction::default(),
        }
    }
    fn background(&self) -> Color {
        self.theme().color(ColorRole::Background)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_app::Application;
    use zui_platform::KeyInput;
    #[test]
    fn controlled_actions_round_trip_through_demo_model_and_tab_order() {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let model = Demo {
            families,
            light: false,
            details: CheckState::Checked,
            mode: RadioOptionId(1),
            level: 50.0,
            input: "ZUI 中文输入".into(),
            smoke: None,
            deadline: None,
            switch_at: None,
        };
        let mut host = WidgetHost::new(
            model,
            zui_layout::TaffyLayout::default(),
            text,
            Default::default(),
        )
        .unwrap();
        let metrics = WindowMetrics {
            extent: zui_types::Extent::new(800, 480),
            scale: 1.0,
            occluded: false,
        };
        let send = |host: &mut WidgetHost<Demo, zui_layout::TaffyLayout, zui_text::ParleyText>,
                    key| {
            host.update(
                WindowEvent::Input(InputEvent::Key(KeyInput {
                    key,
                    pressed: true,
                    repeat: false,
                    text: None,
                    modifiers: Default::default(),
                })),
                metrics,
                Duration::ZERO,
            )
            .unwrap()
        };
        send(&mut host, Key::Tab);
        let button = host.tree().focused().unwrap();
        send(&mut host, Key::Tab);
        let checkbox = host.tree().focused().unwrap();
        send(&mut host, Key::Tab);
        let radio = host.tree().focused().unwrap();
        assert_ne!(button, checkbox);
        assert_ne!(radio, checkbox);
        assert!(send(&mut host, Key::ArrowDown).redraw);
        assert_eq!(host.model().mode, RadioOptionId(2));
        host.draw(&mut zui_paint::Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert_eq!(host.tree().focused(), Some(radio));
        send(&mut host, Key::ArrowUp);
        assert_eq!(host.model().mode, RadioOptionId(1));
        send(&mut host, Key::Tab);
        let slider = host.tree().focused().unwrap();
        assert_ne!(slider, radio);
        assert!(send(&mut host, Key::End).redraw);
        assert_eq!(host.model().level, 100.0);
        send(&mut host, Key::Home);
        assert_eq!(host.model().level, 0.0);
        send(&mut host, Key::Tab);
        let first_step = host.tree().focused().unwrap();
        assert_eq!(
            host.tree().key(first_step).unwrap(),
            Some(&"increment".into())
        );
        send(&mut host, Key::Enter);
        assert_eq!(host.model().level, 5.0);
        host.draw(&mut zui_paint::Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        let bounds = host.tree().layout_box(slider).unwrap().bounds;
        let mut origin = bounds.origin;
        let mut parent = host.tree().parent(slider).unwrap();
        while let Some(id) = parent {
            let local = host.tree().layout_box(id).unwrap().bounds.origin;
            origin.x += local.x;
            origin.y += local.y;
            parent = host.tree().parent(id).unwrap();
        }
        let start = origin.x + 18.0;
        let length = bounds.size.width - 36.0;
        let point = |fraction| {
            zui_types::Point::new(
                start + length * fraction,
                origin.y + bounds.size.height / 2.0,
            )
        };
        host.update(
            WindowEvent::Input(InputEvent::PointerButton {
                button: zui_platform::PointerButton::Primary,
                pressed: true,
                position: Some(point(0.0)),
            }),
            metrics,
            Duration::ZERO,
        )
        .unwrap();
        for (fraction, value) in [(0.7, 70.0), (0.9, 90.0)] {
            host.update(
                WindowEvent::Input(InputEvent::PointerMoved(point(fraction))),
                metrics,
                Duration::ZERO,
            )
            .unwrap();
            assert_eq!(host.model().level, value);
            assert_eq!(host.tree().pointer_capture(), Some(slider));
        }
        host.update(
            WindowEvent::Input(InputEvent::PointerButton {
                button: zui_platform::PointerButton::Primary,
                pressed: false,
                position: Some(point(0.9)),
            }),
            metrics,
            Duration::ZERO,
        )
        .unwrap();
        assert!(host.tree().pointer_capture().is_none());
        send(&mut host, Key::Tab);
        let decrement = host.tree().focused().unwrap();
        assert_ne!(decrement, slider);
        assert!(send(&mut host, Key::Enter).redraw);
        assert_eq!(host.model().level, 85.0);
        assert_eq!(host.tree().focused(), Some(decrement));
        send(&mut host, Key::Tab);
        let increment = host.tree().focused().unwrap();
        assert_ne!(increment, decrement);
        assert!(send(&mut host, Key::Enter).redraw);
        assert_eq!(host.model().level, 90.0);
        assert_eq!(host.tree().focused(), Some(increment));
        host.draw(&mut zui_paint::Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        send(&mut host, Key::Tab);
        let input = host.tree().focused().unwrap();
        assert_ne!(input, slider);
        host.update(
            WindowEvent::Input(InputEvent::Key(KeyInput {
                key: Key::Character("x".into()),
                text: Some("x".into()),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            })),
            metrics,
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(host.model().input, "ZUI 中文输入x");
        host.draw(&mut zui_paint::Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert_eq!(host.tree().focused(), Some(input));
        send(&mut host, Key::Backspace);
        assert_eq!(host.model().input, "ZUI 中文输入");
        send(&mut host, Key::Tab);
        assert_eq!(host.tree().focused(), Some(button));
        assert_eq!(host.model().details, CheckState::Checked);
        assert!(!host.model().light);
        send(&mut host, Key::Enter);
        assert!(host.model().light);
        assert!(host.tree().contains(increment));
        assert!(host.tree().contains(decrement));
        assert_eq!(host.model().level, 90.0);
    }
}
