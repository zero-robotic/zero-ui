use zui_api::{
    layout::{Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Color, Size};
use zui_widgets::{Container, ScrollView, TextView, column};

pub fn scroll_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::dashboard();
    let label =
        |value: String, role| View::new(TextView::themed(&theme, value, families.clone(), role));
    let content = column((1..=12).map(|i| {
        Container::new(label(
            format!("设备 {i:02} · 在线 · 遥测数据已更新"),
            TypeRole::Body,
        ))
        .keyed(i as u64)
        .background(if i % 2 == 0 {
            Color::rgb(15, 47, 62)
        } else {
            Color::rgb(12, 34, 50)
        })
        .style(|s| s.padding = Edges::all(14.0))
        .build()
    }))
    .style(|s| s.gap = Size::new(0.0, 8.0))
    .build();
    let scroll = ScrollView::vertical(content)
        .scroll_to(zui_types::Point::new(0.0, 80.0), 1)
        .background(theme.color(ColorRole::Surface))
        .style(|s| {
            s.height = Dimension::Length(232.0);
            s.padding = Edges::all(12.0);
        })
        .build();
    let view = column([
        label("设备列表 / ScrollView".into(), TypeRole::Heading),
        label(
            "自动内容高度 · 已滚动 80 逻辑像素".into(),
            TypeRole::Caption,
        ),
        scroll,
    ])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Percent(1.0);
        s.padding = Edges::all(24.0);
        s.gap = Size::new(0.0, 12.0);
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

pub fn scroll_both_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::dashboard();
    let label =
        |value: String, role| View::new(TextView::themed(&theme, value, families.clone(), role));
    let content = zui_widgets::grid(
        [zui_api::layout::Track::Length(200.0); 4],
        (0..16).map(|i| {
            Container::new(label(
                format!("区域 {} · 设备 {}\n状态：在线", i / 4 + 1, i % 4 + 1),
                TypeRole::Body,
            ))
            .background(if i % 2 == 0 {
                Color::rgb(15, 47, 62)
            } else {
                Color::rgb(12, 34, 50)
            })
            .style(|s| {
                s.height = Dimension::Length(90.0);
                s.padding = Edges::all(14.0);
            })
            .build()
        }),
    )
    .style(|s| s.gap = Size::new(8.0, 8.0))
    .build();
    let scroll = ScrollView::both(content)
        .scroll_to(zui_types::Point::new(100.0, 50.0), 1)
        .background(theme.color(ColorRole::Surface))
        .style(|s| {
            s.height = Dimension::Length(232.0);
            s.padding = Edges::all(12.0);
        })
        .build();
    let view = column([
        label("设备矩阵 / 双向滚动".into(), TypeRole::Heading),
        label("自然内容尺寸 · 独立双轴滚动条".into(), TypeRole::Caption),
        scroll,
    ])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Percent(1.0);
        s.padding = Edges::all(24.0);
        s.gap = Size::new(0.0, 12.0);
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
