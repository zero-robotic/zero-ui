use std::sync::Arc;
use zui_api::{
    layout::{Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{ColorRole, Space, Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{Container, TextView, column};

pub fn theme_view(theme: &Theme, families: Vec<String>) -> View {
    let mut title = TextView::themed(theme, "ZUI · 语义主题", families.clone(), TypeRole::Heading);
    title.color = theme.color(ColorRole::Accent);
    let body = TextView::themed(
        theme,
        "相同控件使用不同主题。颜色切换只更新绘制，字号与间距变化重新布局。",
        families.clone(),
        TypeRole::Body,
    );
    let mut caption = TextView::themed(
        theme,
        "CPU / GPU · 显式字体 · 无全局主题状态",
        families,
        TypeRole::Caption,
    );
    caption.color = theme.color(ColorRole::TextMuted);
    let panel = Container::new(
        column([
            View::new(body).keyed("body"),
            View::new(caption).keyed("caption"),
        ])
        .style(|s| s.gap = Size::new(0.0, theme.space(Space::Md) as f64))
        .build(),
    )
    .keyed("panel")
    .background(theme.color(ColorRole::Surface))
    .style(|s| s.padding = Edges::all(theme.space(Space::Lg)))
    .build();
    column([View::new(title).keyed("title"), panel])
        .background(theme.color(ColorRole::Background))
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
            s.padding = Edges::all(theme.space(Space::Lg));
            s.gap = Size::new(0.0, theme.space(Space::Lg) as f64);
        })
        .build()
}

pub fn theme_preview(theme: &Theme) -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let mut tree = zui_core::Tree::from_view(theme_view(theme, families), Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    Ok(tree.scene()?)
}
