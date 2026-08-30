use zui_app::Application;
use zui_core::Dip;
use zui_ui::{ColumnLayout, Layout, Padding, ScrollArea, Text, TextWrap, Theme};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = Layout::new(ColumnLayout::new().spacing(Dip(8.0)))
        .child(Text::new("ScrollArea 示例").font_size(4))
        .child(Text::new("将鼠标移到此区域并使用滚轮。内容超出 viewport 时，右侧会显示可拖拽的滚动条。\n\n这是第一段长文本，用于展示自动换行和裁剪。" ).wrap(TextWrap::Word))
        .child(Text::new("第二段：滚动状态属于 ScrollArea，而文本布局与渲染仍由 Text 负责。" ).wrap(TextWrap::Word))
        .child(Text::new("第三段：当所有内容都能显示时，滚动条不会绘制。" ).wrap(TextWrap::Word));
    let root = Padding::new(ScrollArea::vertical(content).height(260.0), Dip(24.0));
    Application::new()
        .title("zero-ui ScrollArea")
        .theme(Theme::default())
        .run(root)
}
