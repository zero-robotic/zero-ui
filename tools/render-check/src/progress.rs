use std::time::Duration;
use zui_api::{
    layout::{Dimension, LayoutStyle},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{ProgressBarView, ProgressValue, TextView, column, row};

pub fn progress_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let (mut text, families) = crate::preview_support::text()?;
    let theme = Theme::dashboard();
    let mut children = vec![View::new(TextView::themed(
        &theme,
        "任务进度 / Progress",
        families.clone(),
        TypeRole::Heading,
    ))];
    for (label, progress, disabled) in [
        ("准备就绪 · 0%", ProgressValue::Determinate(0.0), false),
        ("正在同步 · 35%", ProgressValue::Determinate(0.35), false),
        ("同步完成 · 100%", ProgressValue::Determinate(1.0), false),
        ("正在连接…", ProgressValue::Indeterminate, false),
        ("已暂停 · 65%", ProgressValue::Determinate(0.65), true),
    ] {
        let mut bar = ProgressBarView::themed(&theme, progress);
        bar.disabled = disabled;
        children.push(
            row([
                View::new(TextView::themed(
                    &theme,
                    label,
                    families.clone(),
                    TypeRole::Body,
                ))
                .styled(LayoutStyle {
                    width: Dimension::Length(180.0),
                    ..Default::default()
                }),
                View::new(bar).styled(LayoutStyle {
                    grow: 1.0,
                    height: Dimension::Length(32.0),
                    ..Default::default()
                }),
            ])
            .style(|s| {
                s.gap = Size::new(16.0, 0.0);
            })
            .build(),
        );
    }
    let view = column(children)
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.padding = zui_api::layout::Edges::all(32.0);
            s.gap = Size::new(0.0, 16.0);
        })
        .build();
    let mut tree = zui_core::Tree::from_view(view, Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(640.0, 360.0),
    )?;
    tree.advance_animations(Duration::ZERO)?;
    tree.advance_animations(Duration::from_millis(375))?;
    Ok(tree.scene()?)
}
