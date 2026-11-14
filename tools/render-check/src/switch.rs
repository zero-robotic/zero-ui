use zui_api::{
    action::ActionId,
    layout::{Align, Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{SwitchView, TextView, column, row};
pub fn switch_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::dashboard();
    let mut children = vec![View::new(TextView::themed(
        &theme,
        "系统设置 / Switch",
        families.clone(),
        TypeRole::Heading,
    ))];
    for (label, checked, disabled) in [
        ("自动同步 · 已关闭", false, false),
        ("告警通知 · 已开启", true, false),
        ("远程控制 · 不可修改", true, true),
    ] {
        let mut control = SwitchView::themed(&theme, checked, ActionId(1));
        control.disabled = disabled;
        children.push(
            row([
                View::new(control),
                View::new(TextView::themed(
                    &theme,
                    label,
                    families.clone(),
                    TypeRole::Body,
                )),
            ])
            .style(|s| {
                s.align = Align::Center;
                s.gap = Size::new(24.0, 0.0);
                s.height = Dimension::Length(48.0);
            })
            .build(),
        );
    }
    let view = column(children)
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
    Ok(tree.scene()?)
}
