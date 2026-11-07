use std::sync::Arc;
use zui_api::{
    action::ActionId,
    event::{InputEvent, Key, KeyInput, Modifiers},
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{TextInputView, TextView, column};
pub fn text_input_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let mut children = vec![View::new(TextView::themed(
        &theme,
        "单行输入 / Text input",
        families.clone(),
        TypeRole::Heading,
    ))];
    for (index, (value, disabled, read_only)) in [
        ("中文与 Rust 输入", false, false),
        (
            "长文本会横向滚动，让光标保持可见：组合字符 e\u{301}",
            false,
            false,
        ),
        ("只读内容 · Read only", false, true),
        ("禁用内容 · Disabled", true, false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut input =
            TextInputView::themed(&theme, value, families.clone(), ActionId(index as u64));
        input.disabled = disabled;
        input.read_only = read_only;
        children.push(View::new(input).styled(LayoutStyle {
            height: Dimension::Length(48.0),
            padding: Edges::all(8.0),
            ..Default::default()
        }));
    }
    let view = column(children)
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.padding = Edges::all(32.0);
            s.gap = Size::new(0.0, 12.0);
        })
        .build();
    let mut tree = zui_core::Tree::from_view(view, Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    for (key, modifiers) in [
        (Key::Tab, Modifiers::default()),
        (
            Key::Character("a".into()),
            Modifiers {
                control: true,
                ..Default::default()
            },
        ),
    ] {
        tree.dispatch_input(InputEvent::Key(KeyInput {
            key,
            modifiers,
            pressed: true,
            repeat: false,
            text: None,
        }))?;
    }
    Ok(tree.scene()?)
}
