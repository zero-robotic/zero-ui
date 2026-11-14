use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{SegmentId, SegmentOption, SegmentedControlView, TextView, column};
pub fn segmented_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::dashboard();
    let options = || {
        vec![
            SegmentOption::new(SegmentId(1), "实时监测"),
            SegmentOption::new(SegmentId(2), "历史趋势"),
            SegmentOption::new(SegmentId(3), "告警记录"),
        ]
    };
    let first = SegmentedControlView::themed(
        &theme,
        options(),
        families.clone(),
        Some(SegmentId(1)),
        ActionId(1),
    );
    let mut second = SegmentedControlView::themed(
        &theme,
        options(),
        families.clone(),
        Some(SegmentId(2)),
        ActionId(2),
    );
    second.options[2].disabled = true;
    let label = |value| {
        View::new(TextView::themed(
            &theme,
            value,
            families.clone(),
            TypeRole::Body,
        ))
    };
    let view = column([
        View::new(TextView::themed(
            &theme,
            "视图切换 / Segmented control",
            families.clone(),
            TypeRole::Heading,
        )),
        label("受控选择 · 当前为实时监测"),
        View::new(first),
        label("历史趋势已选中 · 告警记录不可用"),
        View::new(second),
    ])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.padding = Edges::all(32.0);
        s.gap = Size::new(0.0, 18.0);
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
