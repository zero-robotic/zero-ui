use zui_app::Application;
use zui_core::Dip;
use zui_ui::{
    Button, Checkbox, ColumnLayout, IconButton, IconName, Image, ImageFit, ImageId, ImageResource,
    Label, Layout, Padding, Radio, RowLayout, Switch, Text, TextEditor, TextInput, TextView, Theme,
    Widget,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::from_toml_str(include_str!("../theme.toml"))?;
    let email_input = TextInput::new();
    let email_label = Label::new("电子邮箱（点击标签以聚焦输入框）").for_widget(email_input.id());
    let demo_image = ImageResource::new(
        2,
        2,
        vec![
            255, 100, 80, 255, 80, 180, 255, 255, 255, 220, 90, 255, 110, 90, 220, 255,
        ],
    )?;
    let controls = Layout::new(ColumnLayout::new().spacing(Dip(8.0)))
        .child(Text::new("zero-ui controls"))
        .child(IconButton::new(IconName::Menu))
        .child(Button::new("开始语音"))
        .child(Checkbox::new("启用语音识别"))
        .child(Radio::new("普通模式").selected(true))
        .child(Switch::new("自动播放"))
        .child(email_label)
        .child(email_input);
    let content = Layout::new(ColumnLayout::new().spacing(Dip(8.0)))
            .flex(1.0)
            .child(Text::new("Image"))
            .child(
                Image::new(ImageId(1), 2.0, 2.0)
                    .width(280.0)
                    .height(144.0)
                    .fit(ImageFit::Cover),
            )
            .child(Text::new("长文本与滚动"))
            .child(TextView::new("TextView：用于显示可换行、可滚动的大段只读文本。\n\n这一段用于验证长文本在固定 viewport 中的换行、裁剪与滚轮滚动。它会持续重复：Rust UI 的 retained tree、文本布局和资源管理彼此解耦。\n\nRust UI 的 retained tree、文本布局和资源管理彼此解耦。\nRust UI 的 retained tree、文本布局和资源管理彼此解耦。\nRust UI 的 retained tree、文本布局和资源管理彼此解耦。").height(120.0))
            .child(Text::new("多行编辑器"))
            .child(TextEditor::with_text("TextEditor 支持换行；可继续输入更多内容。"));
    let root = Padding::new(
        Layout::new(RowLayout::new().spacing(Dip(32.0)))
            .child(controls)
            .child(content),
        Dip(16.0),
    );
    Application::new()
        .title("zero-ui widgets")
        .theme(theme)
        .image(ImageId(1), demo_image)
        .run(root)
}
