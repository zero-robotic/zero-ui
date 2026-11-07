use std::sync::Arc;
use zui_api::{
    action::ActionId,
    event::{InputEvent, Key, KeyInput},
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_paint::Scene;
use zui_theme::Theme;
use zui_types::Size;
use zui_widgets::{ButtonView, column};

/// Three real controls; the first is focused and armed through the event router.
pub fn button_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let theme = Theme::dashboard();
    let children = [
        ("焦点与按下 · Space", false),
        ("禁用按钮", true),
        ("普通按钮 · Activate", false),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (label, disabled))| {
        let mut button =
            ButtonView::themed(&theme, label, families.clone(), ActionId(index as u64));
        button.disabled = disabled;
        View::new(button).keyed(index as u64).styled(LayoutStyle {
            height: Dimension::Length(64.0),
            padding: Edges::all(12.0),
            ..Default::default()
        })
    });
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
    for key in [Key::Tab, Key::Space] {
        tree.dispatch_input(InputEvent::Key(KeyInput {
            key,
            pressed: true,
            repeat: false,
            text: None,
            modifiers: Default::default(),
        }))?;
    }
    Ok(tree.scene()?)
}
