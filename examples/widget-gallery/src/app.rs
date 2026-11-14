mod advanced;
mod catalog;
mod composition;
mod data_views;
mod overlays;
mod views;
use composition::Extras;
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
    .with_clipboard(zui_app::clipboard::NativeClipboard::default());
    Ok(zui_app::run(config, backend, host)?)
}
#[derive(Clone)]
struct Demo {
    overlays: overlays::State,
    data_views: data_views::State,
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
    Extra(composition::Message),
    Overlays(overlays::Message),
    DataViews(data_views::Message),
    Catalog(catalog::Message),
    Advanced(advanced::Message),
}
impl Demo {
    fn new(families: Vec<String>, smoke: Option<Duration>) -> Self {
        Self {
            extras: Extras::new(),
            overlays: overlays::State::default(),
            data_views: data_views::State::default(),
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
                .overlays
                .map(event)
                .map(Message::Overlays)
                .or_else(|| self.data_views.map(event).map(Message::DataViews))
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
                self.overlays.select_anchor = Some(anchor);
                self.overlays.apply(
                    overlays::Message::Select(zui_widgets::SelectEvent::Open),
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
            Message::DataViews(message) => self.data_views.apply(message),
            Message::Extra(message) => self.extras.message(message),
            Message::Overlays(message) => {
                self.overlays
                    .apply(message, &mut self.level, &mut self.light)
            }
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
                self.overlays.close_anchored();
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
                self.overlays
                    .apply(overlays::Message::Dialog, &mut self.level, &mut self.light);
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
                if let Some(message) = self.overlays.shortcut(&input) {
                    self.overlays
                        .apply(message, &mut self.level, &mut self.light);
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
#[cfg(test)]
mod tests;
