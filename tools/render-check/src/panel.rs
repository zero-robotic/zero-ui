use zui_api::{
    layout::{Dimension, Edges},
    view::View,
};
use zui_extras::charts::{SparklineSeries, SparklineView};
use zui_extras::dashboard::{DashboardPanelView, MetricCardView, MetricChange, MetricTone};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{TextView, column, row};
pub fn panel_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::dashboard();
    let mut cards = Vec::new();
    for (title, value, detail, tone) in [
        ("在线设备", "128", "运行稳定", MetricTone::Positive),
        ("待处理告警", "7", "2 条需关注", MetricTone::Warning),
    ] {
        let mut card = MetricCardView::themed(&theme, title, value, families.clone());
        card.layout.width = Dimension::Length(268.0);
        card.layout.padding = Edges::all(12.0);
        card.layout.gap = Size::new(0.0, 8.0);
        card.change = Some(MetricChange::new(detail, tone));
        cards.push(card.build());
    }
    let mut trend = SparklineView::themed(
        &theme,
        SparklineSeries::new(vec![
            Some(2.0),
            Some(4.0),
            Some(3.0),
            Some(6.0),
            Some(5.0),
            Some(8.0),
            Some(7.0),
            Some(9.0),
        ])?,
    );
    trend.style.natural_size.height = 48.0;
    let panel = DashboardPanelView::themed(
        &theme,
        [
            View::new(TextView::themed(
                &theme,
                "区域运行监测 / ZUI",
                families,
                TypeRole::Heading,
            )),
            row(cards).style(|s| s.gap = Size::new(16.0, 0.0)).build(),
            View::new(trend),
        ],
    )
    .build();
    let view = column([panel])
        .style(|s| {
            s.width = Dimension::Percent(1.0);
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
