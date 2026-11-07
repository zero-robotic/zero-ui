use std::sync::Arc;
use zui_api::{
    action::ActionId,
    event::{InputEvent, Key, KeyInput},
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{RadioGroupView, RadioOption, RadioOptionId, RadioOrientation, TextView, column};

pub fn radio_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    preview(RadioOrientation::Vertical)
}
pub fn radio_horizontal_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    preview(RadioOrientation::Horizontal)
}
fn preview(orientation: RadioOrientation) -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let mut options = vec![
        RadioOption::new(RadioOptionId(1), "自动模式 · Automatic"),
        RadioOption::new(RadioOptionId(2), "维护模式 · Disabled"),
        RadioOption::new(RadioOptionId(3), "手动模式 · Manual"),
    ];
    options[1].disabled = true;
    let mut group = RadioGroupView::themed(
        &theme,
        options,
        families.clone(),
        Some(RadioOptionId(1)),
        ActionId(3),
    );
    group.orientation = orientation;
    let view = column([
        View::new(TextView::themed(
            &theme,
            "运行模式 / Radio group",
            families.clone(),
            TypeRole::Heading,
        )),
        View::new(group).styled(LayoutStyle {
            padding: Edges::all(8.0),
            ..Default::default()
        }),
        View::new(TextView::themed(
            &theme,
            "方向键切换选项 · Space 确认 · 禁用项自动跳过",
            families,
            TypeRole::Caption,
        )),
    ])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.padding = Edges::all(32.0);
        s.gap = Size::new(0.0, 24.0);
    })
    .build();
    let mut tree = zui_core::Tree::from_view(view, Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    tree.dispatch_input(InputEvent::Key(KeyInput {
        key: Key::Tab,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    }))?;
    Ok(tree.scene()?)
}
