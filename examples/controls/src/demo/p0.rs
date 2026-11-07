//! P0 gallery state and actions remain in the application layer.
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges},
    view::View,
};
use zui_theme::{Theme, TypeRole};
use zui_types::{Point, Rect, Size};
use zui_widgets::*;
#[derive(Clone, Default)]
pub(super) struct State {
    open: Open,
    pub status: String,
    secret: String,
    read_only: bool,
}
#[derive(Clone, Default)]
enum Open {
    #[default]
    None,
    Select,
    Menu {
        point: Point,
        path: Vec<MenuId>,
    },
    Dialog,
}
pub(super) enum Message {
    Select(SelectEvent),
    Number(f64),
    InvalidNumber,
    Secret(String),
    ReadOnly(bool),
    Menu(MenuEvent),
    Context(Point),
    Dialog,
    Result(DialogResult),
    Dismiss(DismissReason),
}
impl State {
    pub fn map(&self, e: &ActionEvent) -> Option<Message> {
        match e.action.id {
            ActionId(107) => match e.action.payload::<TextInputEvent>() {
                Some(TextInputEvent::Changed(s)) => Some(Message::Secret(s.clone())),
                _ => None,
            },
            ActionId(108) => e
                .action
                .payload::<CheckChanged>()
                .map(|e| Message::ReadOnly(e.state == CheckState::Checked)),
            ActionId(100) => e
                .action
                .payload::<SelectEvent>()
                .copied()
                .map(Message::Select),
            ActionId(101) => e
                .action
                .payload::<NumberChanged>()
                .map(|e| Message::Number(e.value))
                .or_else(|| {
                    e.action
                        .payload::<NumberRejected>()
                        .map(|_| Message::InvalidNumber)
                }),
            ActionId(102) => e.action.payload::<MenuEvent>().cloned().map(Message::Menu),
            ActionId(103) => e
                .action
                .payload::<ButtonActivated>()
                .map(|_| Message::Dialog),
            ActionId(104) => e
                .action
                .payload::<DialogResult>()
                .copied()
                .map(Message::Result),
            ActionId(105) => e
                .action
                .payload::<PopupDismissed>()
                .map(|e| Message::Dismiss(e.reason)),
            ActionId(106) => e
                .action
                .payload::<ContextMenuRequested>()
                .map(|e| Message::Context(e.position)),
            _ => None,
        }
    }
    pub fn apply(&mut self, m: Message, level: &mut f64, light: &mut bool) {
        match m {
            Message::Secret(value) => self.secret = value,
            Message::ReadOnly(value) => self.read_only = value,
            Message::Select(SelectEvent::Open) => self.open = Open::Select,
            Message::Select(SelectEvent::Selected(id)) => {
                *level = id.0 as f64;
                self.open = Open::None;
                self.status = format!("下拉框设定 {}%", id.0);
            }
            Message::Number(value) => {
                *level = value;
                self.status = format!("数字输入提交 {value}%");
            }
            Message::InvalidNumber => {
                self.status = "无效数值：请输入 0～100，Enter 确认，Escape 恢复".into()
            }
            Message::Dialog => self.open = Open::Dialog,
            Message::Result(result) => {
                if result == DialogResult::Accept {
                    *level = 50.0;
                }
                self.status = format!("弹窗结果：{result:?}");
                self.open = Open::None;
            }
            Message::Menu(MenuEvent::Open(_)) => {
                self.open = Open::Menu {
                    point: Point::new(220.0, 68.0),
                    path: Vec::new(),
                }
            }
            Message::Menu(MenuEvent::OpenPath(path)) => {
                if let Open::Menu { path: p, .. } = &mut self.open {
                    *p = path;
                }
            }
            Message::Menu(MenuEvent::Back) => {
                if let Open::Menu { path, .. } = &mut self.open {
                    path.pop();
                }
            }
            Message::Menu(MenuEvent::Selected(id)) => {
                match id.0 {
                    1 => *level = 25.0,
                    2 => *light = !*light,
                    4 => *level = 100.0,
                    _ => {}
                }
                self.status = format!("执行菜单命令 {}", id.0);
                self.open = Open::None;
            }
            Message::Context(point) => {
                self.open = Open::Menu {
                    point,
                    path: Vec::new(),
                }
            }
            Message::Dismiss(reason) => {
                if reason == DismissReason::Escape
                    && let Open::Menu { path, .. } = &mut self.open
                    && !path.is_empty()
                {
                    path.pop();
                    return;
                }
                self.open = Open::None;
            }
        }
    }
    fn select(&self, theme: &Theme, f: &[String], level: f64) -> SelectView {
        let mut select = SelectView::themed(
            theme,
            [25, 50, 75]
                .into_iter()
                .map(|v| ListItem::new(ListItemId(v), format!("预设 {v}%")))
                .collect(),
            [25, 50, 75]
                .into_iter()
                .find(|v| *v as f64 == level)
                .map(ListItemId),
            f.to_vec(),
            ActionId(100),
        );
        select.placeholder = format!("自定义 {level:.0}%");
        select
    }
    pub fn controls(&self, theme: &Theme, f: &[String], level: f64) -> View {
        let fixed = |mut view: View, width| {
            view.style.width = Dimension::Length(width);
            view.style.height = Dimension::Length(44.0);
            view
        };
        let mut number = NumberInputView::themed(
            theme,
            level,
            NumericRange::new(0.0, 100.0, 5.0).unwrap(),
            f.to_vec(),
            ActionId(101),
        );
        number.precision = 0;
        number.editor.placeholder = "0～100".into();
        let mut secret =
            TextInputView::themed(theme, self.secret.clone(), f.to_vec(), ActionId(107));
        secret.password = true;
        secret.read_only = self.read_only;
        secret.placeholder = "密码：按字素掩码，禁止复制和剪切".into();
        let menu_bar = MenuBarView::themed(
            theme,
            vec![MenuItem::new(MenuId(0), "菜单 / 右键菜单")],
            f.to_vec(),
            ActionId(102),
        )
        .build();
        let dialog_button = View::new(ButtonView::themed(
            theme,
            "打开确认弹窗",
            f.to_vec(),
            ActionId(103),
        ));
        let tools = row([
            fixed(self.select(theme, f, level).build(), 180.0),
            fixed(menu_bar, 180.0),
            fixed(dialog_button, 160.0),
            fixed(View::new(number), 160.0),
        ])
        .style(|s| {
            s.wrap = true;
            s.gap = Size::new(16.0, 12.0);
        })
        .build();
        let read_only = View::new(CheckBoxView::themed(
            theme,
            "只读密码输入",
            f.to_vec(),
            if self.read_only {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            },
            ActionId(108),
        ));
        let inputs = row([fixed(View::new(secret), 360.0), read_only])
            .style(|s| {
                s.wrap = true;
                s.gap = Size::new(16.0, 12.0);
            })
            .build();
        let help = format!(
            "悬停预设按钮查看提示；数字 Enter 提交 / Escape 撤销；右键或 Shift+F10 打开菜单；Ctrl+Shift+R 设为 25%。{}",
            self.status
        );
        column([
            tools,
            inputs,
            View::new(TextView::themed(theme, help, f.to_vec(), TypeRole::Caption)),
        ])
        .style(|s| {
            s.padding = Edges::all(24.0);
            s.gap = Size::new(0.0, 10.0);
            s.shrink = 0.0;
        })
        .build()
    }
    fn menu(theme: &Theme, f: &[String]) -> MenuView {
        let mut preset = MenuItem::new(MenuId(1), "设置为 25%");
        preset.shortcut_label = Some("Ctrl+Shift+R".into());
        preset.shortcut = Some(MenuShortcut {
            key: zui_api::event::Key::Character("R".into()),
            modifiers: zui_api::event::Modifiers {
                control: true,
                shift: true,
                ..Default::default()
            },
        });
        let mut advanced = MenuItem::new(MenuId(3), "更多操作");
        advanced.children = vec![MenuItem::new(MenuId(4), "设置为 100%")];
        let mut disabled = MenuItem::new(MenuId(5), "不可用命令");
        disabled.disabled = true;
        MenuView::themed(
            theme,
            vec![
                preset,
                MenuItem::new(MenuId(2), "切换主题"),
                advanced,
                disabled,
            ],
            f.to_vec(),
            ActionId(102),
        )
    }
    pub fn shortcut(&self, key: &zui_api::event::KeyInput) -> Option<Message> {
        Self::menu(&Theme::dashboard(), &[])
            .resolve_shortcut(key)
            .map(|id| Message::Menu(MenuEvent::Selected(id)))
    }
    pub fn overlay(&self, theme: &Theme, f: &[String], level: f64) -> Option<View> {
        let view = match &self.open {
            Open::None => return None,
            Open::Select => self.select(theme, f, level).popup(
                Rect::new(24.0, 24.0, 180.0, 44.0),
                180.0,
                ActionId(105),
            ),
            Open::Dialog => {
                let message = "确认后所有数值控件和进度展示恢复为 50%。取消或 Escape 保持原值。";
                MessageBoxView::themed(
                    theme,
                    "恢复默认设定？",
                    message,
                    f.to_vec(),
                    ActionId(104),
                    ActionId(105),
                )
                .build()
            }
            Open::Menu { point, path } => {
                let mut menu = Self::menu(theme, f);
                menu.open_path = path.clone();
                menu.popup(PopupPlacement::Point(*point), ActionId(105))
                    .unwrap()
            }
        };
        Some(view)
    }
}
