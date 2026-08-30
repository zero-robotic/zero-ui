use zui_app::Application;
use zui_core::Dip;
use zui_ui::{
    Button, Checkbox, ColumnLayout, IconButton, IconName, Label, Layout, Padding, Radio, Switch,
    Text, TextInput, Theme, Widget,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::from_toml_str(include_str!("../theme.toml"))?;
    let email_input = TextInput::new();
    let email_label = Label::new("电子邮箱（点击标签以聚焦输入框）").for_widget(email_input.id());
    let root = Padding::new(
        Layout::new(ColumnLayout::new().spacing(Dip(8.0)))
            .child(Text::new("zero-ui controls"))
            .child(IconButton::new(IconName::Menu))
            .child(Button::new("开始语音"))
            .child(Checkbox::new("启用语音识别"))
            .child(Radio::new("普通模式").selected(true))
            .child(Switch::new("自动播放"))
            .child(email_label)
            .child(email_input),
        Dip(16.0),
    );
    Application::new()
        .title("zero-ui empty window")
        .theme(theme)
        .run(root)
}
