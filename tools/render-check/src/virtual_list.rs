use std::sync::Arc;
use zui_api::{
    layout::{Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Size};
use zui_widgets::{Container, ScrollAlignment, TextView, VirtualListView, column};

pub fn virtual_list_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    preview(false)
}
pub fn virtual_list_measured_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    preview(true)
}
fn preview(measured: bool) -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let heading = View::new(TextView::themed(
        &theme,
        "遥测记录 / VirtualList",
        families.clone(),
        TypeRole::Heading,
    ));
    let caption = View::new(TextView::themed(
        &theme,
        if measured {
            "10000 条 · 自动行高 · 当前定位第 8001 条"
        } else {
            "100 万条 · 固定行高 · 当前定位第 900001 条"
        },
        families.clone(),
        TypeRole::Caption,
    ));
    let factory = move |index: usize| {
        Container::new(View::new(TextView::themed(
            &theme,
            if measured && index.is_multiple_of(3) {
                format!("#{:07}  设备在线\n温度、压力与流量持续采集中。当前记录包含附加说明，内容按可用宽度自动换行并更新行高。", index + 1)
            } else { format!("#{:07}  设备在线 · 遥测数据正常", index + 1) },
            families.clone(),
            TypeRole::Body,
        )))
        .keyed(index as u64)
        .background(if index.is_multiple_of(2) {
            Color::rgb(15, 47, 62)
        } else {
            Color::rgb(12, 34, 50)
        })
        .style(|s| {
            if !measured { s.height = Dimension::Length(40.0); }
            s.padding = Edges::all(8.0);
        })
        .build()
    };
    let list = if measured {
        VirtualListView::measured(10_000, 48.0, 1, factory)
    } else {
        VirtualListView::new(1_000_000, 40.0, factory)
    }
    .gap(4.0)
    .scroll_to_index(
        if measured { 8000 } else { 900_000 },
        ScrollAlignment::Start,
        1,
    )
    .style(|s| {
        s.height = Dimension::Length(232.0);
        s.padding = Edges::all(8.0);
    })
    .build();
    let view = column([heading, caption, list])
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
