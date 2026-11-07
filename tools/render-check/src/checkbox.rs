use std::sync::Arc;
use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_paint::Scene;
use zui_theme::Theme;
use zui_types::Size;
use zui_widgets::{CheckBoxView, CheckState, column};
pub fn checkbox_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let children = [
        ("未选中 · Unchecked", CheckState::Unchecked, false),
        ("已选中 · Checked", CheckState::Checked, false),
        ("混合状态 · Mixed", CheckState::Mixed, false),
        ("禁用选项 · Disabled", CheckState::Checked, true),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (label, state, disabled))| {
        let mut check = CheckBoxView::themed(
            &theme,
            label,
            families.clone(),
            state,
            ActionId(index as u64),
        );
        check.disabled = disabled;
        View::new(check).keyed(index as u64).styled(LayoutStyle {
            height: Dimension::Length(48.0),
            padding: Edges::all(8.0),
            ..Default::default()
        })
    });
    let view = column(children)
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.padding = Edges::all(32.0);
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
