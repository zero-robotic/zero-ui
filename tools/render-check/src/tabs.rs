use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{ProgressBarView, ProgressValue, TabId, TabOption, TabsView, TextView, column};
pub fn tabs_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::desktop_dark();
    let label = |value, role| View::new(TextView::themed(&theme, value, families.clone(), role));
    let mut disabled = TabOption::new(TabId(3), "历史归档");
    disabled.disabled = true;
    let mut tabs = TabsView::themed(
        &theme,
        vec![
            TabOption::new(TabId(1), "运行概览"),
            TabOption::new(TabId(2), "巡检记录"),
            disabled,
        ],
        families.clone(),
        Some(TabId(1)),
        ActionId(1),
    );
    tabs.layout.gap = Size::new(0.0, 16.0);
    let view = column([
        label("ZUI · 页签工作台", TypeRole::Heading),
        label("按需构建当前页面 · 稳定页签身份", TypeRole::Body),
        tabs.build(|_| {
            column([
                label("运行概览", TypeRole::Heading),
                label("设备在线率  86%", TypeRole::Body),
                View::new(ProgressBarView::themed(
                    &theme,
                    ProgressValue::Determinate(0.86),
                )),
                label("128 台设备在线 / 21 台待巡检", TypeRole::Body),
            ])
            .background(theme.color(ColorRole::Surface))
            .style(|s| {
                s.padding = Edges::all(20.0);
                s.gap = Size::new(0.0, 12.0);
            })
            .build()
        })?,
    ])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.padding = Edges::all(24.0);
        s.gap = Size::new(0.0, 14.0);
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
