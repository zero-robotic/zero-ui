//! Application-owned page data survives tab unmounts; controls remain portable.
use std::{error::Error, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_app::{Backend, ViewAction, ViewApplication, WidgetHost};
use zui_platform::{
    InputEvent, Key, RunReport, WindowAction, WindowConfig, WindowEvent, WindowMetrics,
};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{
    ProgressBarView, ProgressValue, TabChanged, TabId, TabOption, TabsView, TextInputDraft,
    TextInputEvent, TextInputView, TextView, column,
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
    let host = WidgetHost::new(
        Demo::new(families, smoke),
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?
    .with_clipboard(zui_app::clipboard::NativeClipboard::default());
    Ok(zui_app::run(config, backend, host)?)
}
struct Demo {
    families: Vec<String>,
    selected: TabId,
    note: String,
    draft: TextInputDraft,
    smoke: Option<Duration>,
    deadline: Option<Duration>,
    switch_at: Option<Duration>,
}
enum Message {
    Select(TabId),
    Note(String),
}
impl Demo {
    fn new(families: Vec<String>, smoke: Option<Duration>) -> Self {
        Self {
            families,
            selected: TabId(1),
            note: "记录本次巡检结果".into(),
            draft: TextInputDraft::new(),
            smoke,
            deadline: None,
            switch_at: None,
        }
    }
    fn label(&self, theme: &Theme, text: &str, role: TypeRole) -> View {
        View::new(TextView::themed(theme, text, self.families.clone(), role))
    }
    fn page(&self, theme: &Theme, id: TabId) -> View {
        let children = match id {
            TabId(1) => vec![
                self.label(theme, "运行概览", TypeRole::Heading),
                self.label(theme, "设备在线率  86%", TypeRole::Body),
                View::new(ProgressBarView::themed(
                    theme,
                    ProgressValue::Determinate(0.86),
                )),
                self.label(
                    theme,
                    "选择巡检记录填写备注；切回本页后数据仍保存在应用中。",
                    TypeRole::Body,
                ),
            ],
            TabId(2) => vec![
                self.label(theme, "巡检记录", TypeRole::Heading),
                self.label(
                    theme,
                    "编辑备注后切换页签，再返回可继续编辑和撤销。",
                    TypeRole::Body,
                ),
                View::new(
                    TextInputView::themed(
                        theme,
                        self.note.clone(),
                        self.families.clone(),
                        ActionId(2),
                    )
                    .with_draft(self.draft.clone()),
                )
                .keyed("note")
                .styled(LayoutStyle {
                    width: Dimension::Percent(1.0),
                    height: Dimension::Length(48.0),
                    padding: Edges::all(8.0),
                    ..Default::default()
                }),
            ],
            _ => unreachable!("only the two available pages can be selected"),
        };
        column(children)
            .background(theme.color(ColorRole::Surface))
            .style(|s| {
                s.padding = Edges::all(24.0);
                s.gap = Size::new(0.0, 18.0);
            })
            .build()
    }
}
impl ViewApplication for Demo {
    type Message = Message;
    fn view(&self) -> View {
        let theme = Theme::dashboard();
        let mut disabled = TabOption::new(TabId(3), "历史归档");
        disabled.disabled = true;
        let mut tabs = TabsView::themed(
            &theme,
            vec![
                TabOption::new(TabId(1), "运行概览"),
                TabOption::new(TabId(2), "巡检记录"),
                disabled,
            ],
            self.families.clone(),
            Some(self.selected),
            ActionId(1),
        );
        tabs.layout.gap = Size::new(0.0, 16.0);
        column([
            self.label(&theme, "ZUI · 页签工作台", TypeRole::Heading),
            self.label(
                &theme,
                "点击页签，或 Tab 聚焦后用方向键切换。Escape 退出。",
                TypeRole::Body,
            ),
            tabs.build(|id| self.page(&theme, id))
                .expect("static tab IDs are unique"),
        ])
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.padding = Edges::all(32.0);
            s.gap = Size::new(0.0, 20.0);
        })
        .build()
    }
    fn map_action(&self, event: &ActionEvent) -> Option<Message> {
        match event.action.id {
            ActionId(1) => event
                .action
                .payload::<TabChanged>()
                .map(|e| Message::Select(e.selected)),
            ActionId(2) => match event.action.payload::<TextInputEvent>() {
                Some(TextInputEvent::Changed(value)) => Some(Message::Note(value.clone())),
                _ => None,
            },
            _ => None,
        }
    }
    fn message(&mut self, message: Message) -> ViewAction {
        match message {
            Message::Select(id) => self.selected = id,
            Message::Note(value) => self.note = value,
        }
        ViewAction::rebuild()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, elapsed: Duration) -> ViewAction {
        match event {
            WindowEvent::Resumed => {
                self.deadline = self.smoke.map(|duration| elapsed + duration);
                self.switch_at = self.smoke.map(|duration| elapsed + duration / 2);
                ViewAction {
                    window: WindowAction {
                        wake_at: self.switch_at,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }
            WindowEvent::CloseRequested => ViewAction::exit(),
            WindowEvent::Wake if self.deadline.is_some_and(|time| elapsed >= time) => {
                ViewAction::exit()
            }
            WindowEvent::Wake if self.switch_at.is_some_and(|time| elapsed >= time) => {
                self.switch_at = None;
                self.selected = TabId(2);
                ViewAction {
                    rebuild: true,
                    window: WindowAction {
                        wake_at: self.deadline,
                        ..Default::default()
                    },
                }
            }
            WindowEvent::Input(InputEvent::Key(input))
                if input.pressed && input.key == Key::Escape =>
            {
                ViewAction::exit()
            }
            _ => ViewAction::default(),
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
    #[test]
    fn demo_switches_pages_and_releases_and_restores_editing_draft() {
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
            extent: Extent::new(800, 480),
            scale: 1.0,
            occluded: false,
        };
        let key = |key| {
            WindowEvent::Input(InputEvent::Key(KeyInput {
                key,
                pressed: true,
                repeat: false,
                text: None,
                modifiers: Default::default(),
            }))
        };
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert!(!host.model().draft.is_attached());
        host.update(key(Key::Tab), metrics, Duration::ZERO).unwrap();
        host.update(key(Key::ArrowRight), metrics, Duration::ZERO)
            .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert_eq!(host.model().selected, TabId(2));
        assert!(host.model().draft.is_attached());
        // Header focus stays in place, so returning does not require finding page nodes.
        host.update(key(Key::ArrowLeft), metrics, Duration::ZERO)
            .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert!(!host.model().draft.is_attached());
        host.update(key(Key::ArrowRight), metrics, Duration::ZERO)
            .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert!(host.model().draft.is_attached());
        assert_eq!(host.model().note, "记录本次巡检结果");
    }
}
