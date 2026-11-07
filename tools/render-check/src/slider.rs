use std::sync::Arc;
use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_paint::Scene;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{NumericRange, SliderView, TextView, column};
pub fn slider_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let mut children = vec![View::new(TextView::themed(
        &theme,
        "设定值 / Slider",
        families.clone(),
        TypeRole::Heading,
    ))];
    for (index, (value, disabled)) in [(25.0, false), (65.0, false), (80.0, true)]
        .into_iter()
        .enumerate()
    {
        let mut slider = SliderView::themed(
            &theme,
            NumericRange::new(0.0, 100.0, 5.0)?,
            value,
            ActionId(index as u64),
        );
        slider.disabled = disabled;
        children.push(View::new(TextView::themed(
            &theme,
            format!("{value:.0}%{}", if disabled { " · 禁用" } else { "" }),
            families.clone(),
            TypeRole::Caption,
        )));
        children.push(View::new(slider).styled(LayoutStyle {
            height: Dimension::Length(36.0),
            padding: Edges::all(8.0),
            ..Default::default()
        }));
    }
    let view = column(children)
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.padding = Edges::all(32.0);
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
