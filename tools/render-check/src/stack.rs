use zui_api::layout::{Align, Dimension, Edges};
use zui_api::view::View;
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Size};
use zui_widgets::{TextView, column, stack};

/// Shared-area layers and independent anchors, without absolute positioning.
pub fn stack_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::dashboard();
    let label =
        |value: &str, role| View::new(TextView::themed(&theme, value, families.clone(), role));
    let base = column([
        label("叠层布局 / Stack", TypeRole::Heading),
        label("共享内容区域 · 独立对齐", TypeRole::Body),
    ])
    .background(Color::rgb(12, 32, 50))
    .style(|s| {
        s.padding = Edges::all(20.0);
        s.gap = Size::new(0.0, 8.0);
    })
    .build();
    let panel = column([
        label("数据面板", TypeRole::Heading),
        label("居中叠放，保留子项布局", TypeRole::Body),
    ])
    .background(Color::rgb(22, 65, 82))
    .style(|s| {
        s.width = Dimension::Length(300.0);
        s.height = Dimension::Length(112.0);
        s.padding = Edges::all(20.0);
        s.gap = Size::new(0.0, 12.0);
        s.align_self = Some(Align::Center);
        s.justify_self = Some(Align::Center);
    })
    .build();
    let badge = column([label("在线", TypeRole::Body)])
        .background(Color::rgb(20, 100, 100))
        .style(|s| {
            s.padding = Edges::all(10.0);
            s.margin = Edges::all(16.0);
            s.align_self = Some(Align::Start);
            s.justify_self = Some(Align::End);
        })
        .build();
    let footer = column([label("角标与面板共享同一容器", TypeRole::Body)])
        .style(|s| {
            s.margin = Edges::all(20.0);
            s.align_self = Some(Align::End);
            s.justify_self = Some(Align::End);
        })
        .build();
    let view = stack([base, panel, badge, footer])
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
            s.padding = Edges::all(24.0);
        })
        .build();
    let mut tree = zui_core::Tree::from_view(view, Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    Ok(tree.scene()?)
}
