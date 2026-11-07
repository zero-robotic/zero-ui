use std::sync::Arc;
use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{
    AccordionExpansion, AccordionId, AccordionItem, AccordionView, ProgressBarView, ProgressValue,
    TextView, column, row,
};

pub fn accordion_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let label = |value, role| View::new(TextView::themed(&theme, value, families.clone(), role));
    let group = |multiple| -> Result<View, zui_widgets::AccordionError> {
        let mut disabled = AccordionItem::new(AccordionId(3), "归档设置（禁用）");
        disabled.disabled = true;
        let mut accordion = AccordionView::themed(
            &theme,
            vec![
                AccordionItem::new(AccordionId(1), "设备概况"),
                AccordionItem::new(AccordionId(2), "采集参数"),
                disabled,
            ],
            families.clone(),
            if multiple {
                AccordionExpansion::Multiple([AccordionId(1), AccordionId(2)].into())
            } else {
                AccordionExpansion::Single(Some(AccordionId(1)))
            },
            ActionId(1),
        );
        accordion.layout.gap = Size::new(0.0, 8.0);
        accordion.header_layout.padding = Edges::all(6.0);
        accordion.build(|id| {
            column([
                label(
                    if id == AccordionId(1) {
                        "在线设备  128 / 149"
                    } else {
                        "采样周期  100 ms"
                    },
                    TypeRole::Body,
                ),
                View::new(ProgressBarView::themed(
                    &theme,
                    ProgressValue::Determinate(if id == AccordionId(1) { 0.86 } else { 0.40 }),
                )),
            ])
            .background(theme.color(ColorRole::Surface))
            .style(|s| {
                s.padding = Edges::all(8.0);
                s.gap = Size::new(0.0, 8.0);
            })
            .build()
        })
    };
    let view = column([
        label("ZUI · 分组设置", TypeRole::Heading),
        row([
            column([label("单项展开", TypeRole::Body), group(false)?])
                .style(|s| {
                    s.width = Dimension::Length(288.0);
                    s.gap = Size::new(0.0, 8.0);
                })
                .build(),
            column([label("多项展开", TypeRole::Body), group(true)?])
                .style(|s| {
                    s.width = Dimension::Length(288.0);
                    s.gap = Size::new(0.0, 8.0);
                })
                .build(),
        ])
        .style(|s| s.gap = Size::new(24.0, 0.0))
        .build(),
    ])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.padding = Edges::all(20.0);
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
