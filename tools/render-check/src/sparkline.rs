use std::sync::Arc;
use zui_api::{
    layout::{Dimension, Edges},
    view::View,
};
use zui_extras::charts::{SparklineSeries, SparklineView};
use zui_paint::Scene;
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{TextView, column, row};
pub fn sparkline_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let label = |value, role| View::new(TextView::themed(&theme, value, families.clone(), role));
    let mut cards = Vec::new();
    for (title, caption, values) in [
        (
            "设备负载",
            "趋势 + 面积",
            vec![
                Some(2.0),
                Some(4.0),
                Some(3.0),
                Some(6.0),
                Some(5.0),
                Some(8.0),
                Some(7.0),
                Some(9.0),
            ],
        ),
        (
            "链路监测",
            "缺测处保留断线",
            vec![
                Some(1.0),
                Some(4.0),
                Some(3.0),
                None,
                None,
                Some(5.0),
                Some(2.0),
                Some(4.0),
            ],
        ),
        ("稳态读数", "常量序列居中", vec![Some(5.0); 8]),
    ] {
        let mut chart = SparklineView::themed(&theme, SparklineSeries::new(values)?);
        chart.style.natural_size = Size::new(160.0, 112.0);
        cards.push(
            column([
                label(title, TypeRole::Body),
                View::new(chart),
                label(caption, TypeRole::Body),
            ])
            .background(theme.color(ColorRole::Surface))
            .style(|s| {
                s.width = Dimension::Length(186.0);
                s.padding = Edges::all(12.0);
                s.gap = Size::new(0.0, 16.0);
            })
            .build(),
        );
    }
    let view = column([
        label("ZUI · 实时趋势", TypeRole::Heading),
        label("独立图表模块 / 同一绘制描述支持 CPU 与 GPU", TypeRole::Body),
        row(cards).style(|s| s.gap = Size::new(17.0, 0.0)).build(),
    ])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.padding = Edges::all(24.0);
        s.gap = Size::new(0.0, 16.0);
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
