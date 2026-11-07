mod advanced;
mod catalog;
mod more;
mod p0;
mod p1;
use more::Extras;
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
    let model = Demo::new(families, smoke);
    let host = WidgetHost::new(
        model,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?
    .with_clipboard(zui_clipboard::NativeClipboard::default());
    Ok(zui_app::run(config, backend, host)?)
}
#[derive(Clone)]
struct Demo {
    p0: p0::State,
    p1: p1::State,
    catalog: catalog::State,
    advanced: advanced::State,
    extras: Extras,
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
    OpenSelect(zui_types::Rect),
    OpenCombo(zui_types::Rect),
    OpenDate(bool, zui_types::Rect),
    ToggleTheme,
    SetDetails(CheckState),
    SetMode(RadioOptionId),
    SetLevel(f64),
    SetInput(String),
    Extra(more::Message),
    P0(p0::Message),
    P1(p1::Message),
    Catalog(catalog::Message),
    Advanced(advanced::Message),
}
impl Demo {
    fn new(families: Vec<String>, smoke: Option<Duration>) -> Self {
        Self {
            extras: Extras::new(),
            p0: p0::State::default(),
            p1: p1::State::default(),
            catalog: catalog::State::default(),
            advanced: advanced::State::default(),
            families,
            light: false,
            details: CheckState::Checked,
            mode: RadioOptionId(1),
            level: 50.0,
            input: "ZUI 中文输入".into(),
            smoke,
            deadline: None,
            switch_at: None,
        }
    }

    fn theme(&self) -> Theme {
        if self.light {
            Theme::desktop_light()
        } else {
            Theme::desktop_dark()
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
            _ => self
                .p0
                .map(event)
                .map(Message::P0)
                .or_else(|| self.p1.map(event).map(Message::P1))
                .or_else(|| self.catalog.map(event).map(Message::Catalog))
                .or_else(|| self.advanced.map(event).map(Message::Advanced))
                .or_else(|| self.extras.map_action(event).map(Message::Extra)),
        }
    }
    fn map_action_with_tree(&self, event: &ActionEvent, tree: &zui_core::Tree) -> Option<Message> {
        let select = event.action.id == ActionId(100)
            && event.action.payload::<zui_widgets::SelectEvent>()
                == Some(&zui_widgets::SelectEvent::Open);
        let combo = event.action.id == ActionId(400)
            && event.action.payload::<zui_widgets::ComboBoxEvent>()
                == Some(&zui_widgets::ComboBoxEvent::Open);
        let date = matches!(event.action.id, ActionId(302) | ActionId(303))
            && event.action.payload::<zui_widgets::DateEvent>()
                == Some(&zui_widgets::DateEvent::Open);
        if select || combo || date {
            let mut anchor = tree.presented_bounds(event.source).ok()?;
            let root = tree.presented_bounds(tree.root()).ok()?;
            anchor.origin.x -= root.origin.x;
            anchor.origin.y -= root.origin.y;
            return Some(if select {
                Message::OpenSelect(anchor)
            } else if date {
                Message::OpenDate(event.action.id == ActionId(302), anchor)
            } else {
                Message::OpenCombo(anchor)
            });
        }
        self.map_action(event)
    }
    fn message(&mut self, message: Message) -> ViewAction {
        match message {
            Message::OpenSelect(anchor) => {
                self.p0.select_anchor = Some(anchor);
                self.p0.apply(
                    p0::Message::Select(zui_widgets::SelectEvent::Open),
                    &mut self.level,
                    &mut self.light,
                );
            }
            Message::OpenCombo(anchor) => {
                self.advanced.anchor = Some(anchor);
                self.advanced
                    .apply(advanced::Message::Combo(zui_widgets::ComboBoxEvent::Open));
            }
            Message::OpenDate(start, anchor) => self.catalog.open_date(start, anchor),
            Message::ToggleTheme => self.light = !self.light,
            Message::SetDetails(state) => self.details = state,
            Message::SetMode(mode) => self.mode = mode,
            Message::SetLevel(level) => self.level = level,
            Message::SetInput(text) => self.input = text,
            Message::Catalog(message) => self.catalog.apply(message),
            Message::Advanced(message) => self.advanced.apply(message),
            Message::P1(message) => self.p1.apply(message),
            Message::Extra(message) => self.extras.message(message),
            Message::P0(message) => self.p0.apply(message, &mut self.level, &mut self.light),
        }
        println!(
            "control message: light={}, details={:?}, mode={:?}, level={}",
            self.light, self.details, self.mode, self.level
        );
        ViewAction::rebuild()
    }
    fn view(&self) -> View {
        let snapshot = self.clone();
        zui_widgets::TooltipView::themed(
            &self.theme(),
            self.families.clone(),
            vec![zui_widgets::TooltipAnchor {
                bounds: zui_types::Rect::new(24.0, 24.0, 180.0, 44.0),
                text: "选择预设后，数值控件与进度展示会同步更新。".into(),
            }],
            move || snapshot.gallery(),
        )
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
            WindowEvent::Resized | WindowEvent::ScaleChanged => {
                self.p0.close_anchored();
                self.advanced.close_anchored();
                self.catalog.apply(catalog::Message::CloseDate);
                ViewAction::rebuild()
            }
            WindowEvent::CloseRequested => ViewAction::exit(),
            WindowEvent::Wake if self.deadline.is_some_and(|time| elapsed >= time) => {
                ViewAction::exit()
            }
            WindowEvent::Wake if self.switch_at.is_some_and(|time| elapsed >= time) => {
                self.switch_at = None;
                self.light = !self.light;
                self.p0
                    .apply(p0::Message::Dialog, &mut self.level, &mut self.light);
                println!("widget smoke: theme switched and modal opened");
                ViewAction {
                    window: WindowAction {
                        wake_at: self.deadline,
                        ..Default::default()
                    },
                    rebuild: true,
                }
            }
            WindowEvent::Input(InputEvent::Key(input)) if input.pressed && !input.repeat => {
                if let Some(message) = self.p0.shortcut(&input) {
                    self.p0.apply(message, &mut self.level, &mut self.light);
                    return ViewAction::rebuild();
                }
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

impl Demo {
    fn gallery(&self) -> View {
        let theme = self.theme();
        let scroll = zui_widgets::ScrollView::vertical(
            column([
                self.advanced.view(&theme, &self.families),
                self.catalog.view(&theme, &self.families),
                self.p1.view(&theme, &self.families),
                self.basic_view(),
                self.extras
                    .view(&theme, &self.families, &self.input, self.level),
            ])
            .style(|s| s.gap = Size::new(0.0, 20.0))
            .build(),
        )
        .style(|s| {
            s.grow = 1.0;
            s.basis = Dimension::Length(0.0);
            s.min_height = Dimension::Length(0.0);
            s.width = Dimension::Percent(1.0);
        })
        .build();
        let base = column([self.p0.controls(&theme, &self.families, self.level), scroll])
            .style(|s| {
                s.width = Dimension::Percent(1.0);
                s.height = Dimension::Percent(1.0);
            })
            .build();
        let mut layers = vec![
            zui_widgets::ContextMenuRegion {
                action: ActionId(106),
            }
            .build(base)
            .keyed("base"),
        ];
        if let Some(overlay) = self.p0.overlay(&theme, &self.families, self.level) {
            layers.push(overlay.keyed("popup"));
        }
        if let Some(overlay) = self.advanced.overlay(&theme, &self.families) {
            layers.push(overlay.keyed("advanced-overlay"));
        }
        if let Some(overlay) = self.catalog.overlay(&theme, &self.families) {
            layers.push(overlay.keyed("catalog-overlay"));
        }
        zui_widgets::stack(layers)
            .style(|s| {
                s.width = Dimension::Percent(1.0);
                s.height = Dimension::Percent(1.0);
            })
            .build()
    }
    fn basic_view(&self) -> View {
        let theme = self.theme();
        let mut title = TextView::themed(
            &theme,
            "ZUI · 传统控件展厅",
            self.families.clone(),
            TypeRole::Heading,
        );
        title.color = theme.color(ColorRole::Accent);
        let body = TextView::themed(
            &theme,
            "Slider / Stepper 同步进度条；输入文字在下方页签回显。向下滚动查看全部控件，Tab 切换焦点，Escape 退出。",
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
                        min_height: Dimension::Length(28.0),
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
                        min_height: Dimension::Length(28.0),
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
                View::new({
                    let mut input = TextInputView::themed(
                        &theme,
                        self.input.clone(),
                        self.families.clone(),
                        ActionId(5),
                    );
                    input.placeholder = "输入后同步到下方摘要".into();
                    input
                })
                .keyed("input")
                .styled(zui_api::layout::LayoutStyle {
                    width: Dimension::Length(320.0),
                    min_height: Dimension::Length(28.0),
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
            s.padding = Edges::all(theme.space(Space::Lg));
            s.gap = Size::new(0.0, theme.space(Space::Lg) as f64);
        })
        .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_api::action::WidgetAction;
    use zui_widgets::*;
    #[test]
    fn all_gallery_states_layout_and_actions_round_trip() {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut model = Demo::new(families, None);
        let mut tree = zui_core::Tree::from_view(model.view(), Default::default()).unwrap();
        let actions = [
            WidgetAction::new(ActionId(300), ButtonActivated),
            WidgetAction::new(ActionId(300), ButtonActivated),
            WidgetAction::new(ActionId(312), NotificationDismissed { id: 2 }),
            WidgetAction::new(ActionId(100), SelectEvent::Open),
            WidgetAction::new(ActionId(100), SelectEvent::Selected(ListItemId(75))),
            WidgetAction::new(ActionId(101), NumberChanged { value: 35.0 }),
            WidgetAction::new(ActionId(103), ButtonActivated),
            WidgetAction::new(ActionId(104), DialogResult::Accept),
            WidgetAction::new(ActionId(102), MenuEvent::Open(MenuId(0))),
            WidgetAction::new(ActionId(102), MenuEvent::OpenPath(vec![MenuId(3)])),
            WidgetAction::new(
                ActionId(105),
                PopupDismissed {
                    reason: DismissReason::Escape,
                },
            ),
            WidgetAction::new(ActionId(102), MenuEvent::Selected(MenuId(1))),
            WidgetAction::new(ActionId(4), SliderChanged { value: 75.0 }),
            WidgetAction::new(ActionId(6), StepperChanged { value: 80.0 }),
            WidgetAction::new(ActionId(5), TextInputEvent::Changed("同步内容".into())),
            WidgetAction::new(ActionId(10), ToggleChanged { selected: false }),
            WidgetAction::new(ActionId(11), SwitchChanged { checked: true }),
            WidgetAction::new(ActionId(12), DisclosureChanged { expanded: false }),
            WidgetAction::new(
                ActionId(13),
                SegmentChanged {
                    selected: SegmentId(2),
                },
            ),
            WidgetAction::new(
                ActionId(14),
                ListSelectionChanged {
                    selected: ListItemId(3),
                },
            ),
            WidgetAction::new(ActionId(15), TabChanged { selected: TabId(1) }),
            WidgetAction::new(
                ActionId(16),
                AccordionChanged {
                    item: AccordionId(2),
                    expansion: AccordionExpansion::Single(Some(AccordionId(2))),
                },
            ),
            WidgetAction::new(ActionId(1), ButtonActivated),
        ];
        for action in actions {
            let message = model
                .map_action(&ActionEvent {
                    source: tree.root(),
                    action,
                })
                .expect("public action maps to app message");
            assert!(model.message(message).rebuild);
            tree.reconcile(model.view()).unwrap();
            for size in [Size::new(1100.0, 800.0), Size::new(480.0, 640.0)] {
                tree.layout_with_text(&mut zui_layout::TaffyLayout::default(), &mut text, size)
                    .unwrap();
                tree.scene().unwrap().validate(Default::default()).unwrap();
            }
        }
        assert_eq!(model.level, 80.0);
        assert_eq!(model.input, "同步内容");
        assert!(model.light);
    }
}

#[cfg(test)]
mod p0_host_tests {
    use super::*;
    use zui_app::Application;
    use zui_platform::{KeyInput, Modifiers, PointerButton};
    #[test]
    fn pointer_and_keyboard_drive_select_dialog_number_and_submenu_round_trips() {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut host = WidgetHost::new(
            Demo::new(families, None),
            zui_layout::TaffyLayout::default(),
            text,
            Default::default(),
        )
        .unwrap();
        let metrics = WindowMetrics {
            extent: zui_types::Extent::new(1100, 800),
            scale: 1.0,
            occluded: false,
        };
        let mut scene = zui_paint::Scene::new();
        host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
        let send = |host: &mut WidgetHost<Demo, zui_layout::TaffyLayout, zui_text::ParleyText>,
                    input| {
            host.update(WindowEvent::Input(input), metrics, Duration::ZERO)
                .unwrap();
            host.draw(&mut zui_paint::Scene::new(), metrics, Duration::ZERO)
                .unwrap();
        };
        let click =
            |host: &mut WidgetHost<Demo, zui_layout::TaffyLayout, zui_text::ParleyText>, x, y| {
                let point = zui_types::Point::new(x, y);
                send(host, InputEvent::PointerMoved(point));
                for pressed in [true, false] {
                    send(
                        host,
                        InputEvent::PointerButton {
                            button: PointerButton::Primary,
                            pressed,
                            position: Some(point),
                        },
                    );
                }
            };
        let key = |key, text, modifiers| {
            InputEvent::Key(KeyInput {
                key,
                text,
                modifiers,
                pressed: true,
                repeat: false,
            })
        };
        click(&mut host, 40.0, 40.0);
        let popup_focus = host.tree().focused();
        send(&mut host, key(Key::ArrowDown, None, Modifiers::default()));
        assert_eq!(host.model().level, 50.0);
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 75.0);
        assert_ne!(host.tree().focused(), popup_focus);
        click(&mut host, 450.0, 40.0);
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 50.0);
        click(&mut host, 620.0, 40.0);
        send(
            &mut host,
            key(
                Key::Character("a".into()),
                None,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            ),
        );
        send(
            &mut host,
            key(
                Key::Character("60".into()),
                Some("60".into()),
                Modifiers::default(),
            ),
        );
        assert_eq!(host.model().level, 50.0);
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 60.0);
        click(&mut host, 240.0, 40.0);
        send(&mut host, key(Key::End, None, Modifiers::default()));
        send(&mut host, key(Key::ArrowRight, None, Modifiers::default()));
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 100.0);
        click(&mut host, 450.0, 40.0);
        let shortcut = || {
            key(
                Key::Character("R".into()),
                None,
                Modifiers {
                    control: true,
                    shift: true,
                    ..Default::default()
                },
            )
        };
        send(&mut host, shortcut());
        assert_eq!(
            host.model().level,
            100.0,
            "modal blocks background shortcuts"
        );
        send(&mut host, key(Key::Escape, None, Modifiers::default()));
        send(&mut host, shortcut());
        assert_eq!(host.model().level, 25.0);
    }
}

#[cfg(test)]
mod anchor_tests {
    use super::*;
    use zui_app::Application;
    use zui_types::{NodeId, Point};
    fn find(tree: &zui_core::Tree, key: &str) -> NodeId {
        let mut pending = vec![tree.root()];
        while let Some(id) = pending.pop() {
            if tree.key(id).unwrap() == Some(&zui_api::view::Key::from(key)) {
                return id;
            }
            pending.extend_from_slice(tree.children(id).unwrap());
        }
        panic!("missing {key}");
    }
    #[test]
    fn date_popup_uses_the_activated_start_or_end_button_bounds() {
        use zui_api::action::WidgetAction;
        use zui_widgets::{Date, DateEvent, DatePickerView};
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        for action in [302, 303] {
            let mut demo = Demo::new(families.clone(), None);
            let theme = demo.theme();
            let trigger = DatePickerView::themed(
                &theme,
                Date::new(2026, 11, 1).unwrap(),
                families.clone(),
                ActionId(action),
            )
            .build()
            .unwrap()
            .keyed("date-trigger");
            let view = column([trigger])
                .style(|s| {
                    s.padding = Edges::all(40.0);
                    s.width = Dimension::Length(240.0);
                })
                .build();
            let mut tree = zui_core::Tree::from_view(view, Default::default()).unwrap();
            tree.layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut text,
                Size::new(800.0, 800.0),
            )
            .unwrap();
            let source = find(&tree, "date-trigger");
            let button = tree.presented_bounds(source).unwrap();
            let message = demo
                .map_action_with_tree(
                    &ActionEvent {
                        source,
                        action: WidgetAction::new(ActionId(action), DateEvent::Open),
                    },
                    &tree,
                )
                .unwrap();
            demo.message(message);
            let overlay = demo.catalog.overlay(&theme, &families).unwrap();
            let mut popup = zui_core::Tree::from_view(overlay, Default::default()).unwrap();
            popup
                .layout_with_text(
                    &mut zui_layout::TaffyLayout::default(),
                    &mut text,
                    Size::new(800.0, 800.0),
                )
                .unwrap();
            let bounds = popup
                .presented_bounds(popup.children(popup.root()).unwrap()[0])
                .unwrap();
            assert!((bounds.origin.x - button.origin.x).abs() < 0.01);
            assert!((bounds.origin.y - button.origin.y - button.size.height).abs() < 0.01);
            demo.catalog.apply(catalog::Message::CloseDate);
            assert!(demo.catalog.overlay(&theme, &families).is_none());
        }
    }
    #[test]
    fn dropdowns_anchor_to_actual_buttons_after_layout_and_scroll() {
        for (width, scale) in [(1100, 1.0), (1280, 2.0)] {
            let mut text = zui_text::ParleyText::default();
            let families = text
                .register_font(Arc::from(
                    include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
                ))
                .unwrap();
            let mut host = WidgetHost::new(
                Demo::new(families, None),
                zui_layout::TaffyLayout::default(),
                text,
                Default::default(),
            )
            .unwrap();
            let metrics = WindowMetrics {
                extent: zui_types::Extent::new(width, (1000.0 * scale) as u32),
                scale,
                occluded: false,
            };
            let mut scene = zui_paint::Scene::new();
            host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
            for (button_key, overlay_key) in [
                ("preset-select", "popup"),
                ("device-combo", "advanced-overlay"),
            ] {
                let id = find(host.tree(), button_key);
                if button_key == "device-combo" {
                    let before = host.tree().presented_bounds(id).unwrap();
                    host.update(
                        WindowEvent::Input(InputEvent::PointerMoved(Point::new(
                            before.origin.x + 5.0,
                            before.origin.y + 5.0,
                        ))),
                        metrics,
                        Duration::ZERO,
                    )
                    .unwrap();
                    host.update(
                        WindowEvent::Input(InputEvent::Scroll(zui_platform::ScrollDelta::Pixels {
                            x: 0.0,
                            y: -10.0,
                        })),
                        metrics,
                        Duration::ZERO,
                    )
                    .unwrap();
                    host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
                    assert!(host.tree().presented_bounds(id).unwrap().origin.y < before.origin.y);
                }
                let button = host.tree().presented_bounds(id).unwrap();
                let point = Point::new(button.origin.x + 5.0, button.origin.y + 5.0);
                for pressed in [true, false] {
                    host.update(
                        WindowEvent::Input(InputEvent::PointerButton {
                            button: zui_platform::PointerButton::Primary,
                            pressed,
                            position: Some(point),
                        }),
                        metrics,
                        Duration::ZERO,
                    )
                    .unwrap();
                }
                host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
                let overlay = find(host.tree(), overlay_key);
                let content = host.tree().children(overlay).unwrap()[0];
                let popup = host.tree().presented_bounds(content).unwrap();
                assert!(
                    (popup.origin.x - button.origin.x).abs() < 0.01,
                    "{button_key}: {button:?} vs {popup:?}"
                );
                assert!(
                    (popup.origin.y - button.origin.y - button.size.height).abs() < 0.01,
                    "{button_key}: {button:?} vs {popup:?}"
                );
                assert!((popup.size.width - button.size.width).abs() < 0.01);
                let action = host
                    .update(WindowEvent::Resized, metrics, Duration::ZERO)
                    .unwrap();
                assert!(action.redraw);
                host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
                assert!(!host.tree().contains(overlay));
            }
        }
    }
}
