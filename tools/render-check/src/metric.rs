use std::sync::Arc;
use zui_api::layout::Dimension;
use zui_api::{layout::Edges, view::View};
use zui_extras::charts::{SparklineSeries, SparklineView};
use zui_extras::dashboard::{MetricCardView, MetricChange, MetricTone};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{TextView, column, row};
pub fn metric_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let mut cards = Vec::new();
    for (title, value, unit, change, tone, data) in [
        (
            "在线设备",
            "128",
            "台",
            "较昨日 +12 台",
            MetricTone::Positive,
            vec![3.0, 5.0, 4.0, 7.0, 6.0, 9.0],
        ),
        (
            "平均延迟",
            "24",
            "ms",
            "延迟下降 8 ms",
            MetricTone::Positive,
            vec![9.0, 7.0, 8.0, 6.0, 4.0, 3.0],
        ),
        (
            "待处理告警",
            "7",
            "条",
            "其中 2 条需关注",
            MetricTone::Warning,
            vec![2.0, 3.0, 2.0, 5.0, 4.0, 6.0],
        ),
    ] {
        let mut card = MetricCardView::themed(&theme, title, value, families.clone());
        card.unit = Some(unit.into());
        card.change = Some(MetricChange::new(change, tone));
        let mut trend = SparklineView::themed(
            &theme,
            SparklineSeries::new(data.into_iter().map(Some).collect())?,
        );
        trend.style.natural_size.height = 70.0;
        card.trend = Some(trend);
        card.layout.width = Dimension::Length(186.0);
        cards.push(card.build());
    }
    let view = column([
        View::new(TextView::themed(
            &theme,
            "ZUI · 运行指标",
            families.clone(),
            TypeRole::Heading,
        )),
        View::new(TextView::themed(
            &theme,
            "指标卡组合 / 数据与业务含义由应用提供",
            families,
            TypeRole::Body,
        )),
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
