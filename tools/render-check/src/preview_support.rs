use std::sync::Arc;

/// Use the bundled font for deterministic previews without system font I/O.
pub(crate) fn text() -> Result<(zui_text::ParleyText, Vec<String>), zui_api::text::TextError> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    Ok((text, families))
}
