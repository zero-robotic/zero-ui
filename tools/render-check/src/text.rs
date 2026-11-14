use zui_api::text::{TextAlign, TextError, TextRequest, TextService, TextStyle, TextWidth};
use zui_paint::{Path, Scene};
use zui_types::{Affine, Color, Rect};

/// Exercises the public control, tree, injected services and final layout preparation.
pub fn text_widget_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    use zui_api::{
        layout::{Dimension, Edges, LayoutStyle},
        view::View,
    };
    let (mut text, families) = crate::preview_support::text()?;
    let view = View::new(zui_widgets::TextView::new(
        "ZUI · 文本控件\n\n文字服务通过公共契约注入。布局测量支持中文与 Rust 混排，最终绘制遵循内容宽度和内边距。\n\nCPU / GPU 共享控件与排版结果。",
        TextStyle { families, font_size: 26.0, line_height: 1.35, align: TextAlign::Start },
        Color::rgb(41, 210, 230),
    )).styled(LayoutStyle {
        width: Dimension::Percent(1.0), padding: Edges::all(28.0), ..Default::default()
    });
    let mut tree = zui_core::Tree::from_view(view, Default::default())?;
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        zui_types::Size::new(640.0, 360.0),
    )?;
    Ok(tree.scene()?)
}

/// The service is dropped before rendering; Scene owns every referenced font.
pub fn text_preview() -> Result<Scene, TextError> {
    let (mut text, families) = crate::preview_support::text()?;
    let mut style = TextStyle {
        families,
        font_size: 34.0,
        line_height: 1.35,
        align: TextAlign::Start,
    };
    let title = text.layout(TextRequest {
        text: "ZUI · 中文界面",
        style: &style,
        width: TextWidth::Unbounded,
    })?;
    style.font_size = 22.0;
    let paragraph =
        "同一排版结果用于测量与绘制。窗口变窄时，文字会按可用宽度重新换行。Rust + Vello CPU / GPU";
    let left = text.layout(TextRequest {
        text: paragraph,
        style: &style,
        width: TextWidth::AtMost(340.0),
    })?;
    style.font_size = 18.0;
    style.align = TextAlign::Center;
    let right = text.layout(TextRequest {
        text: "资源共享\n字体持有\ne\u{301} · 123",
        style: &style,
        width: TextWidth::AtMost(160.0),
    })?;
    let mut scene = Scene::new();
    let mut painter = scene.painter();
    painter.with_transform(Affine::translate(28.0, 22.0), |painter| {
        title.paint(painter, Color::rgb(41, 210, 230))
    });
    painter.fill_rect(Rect::new(28.0, 88.0, 584.0, 1.0), Color::rgb(35, 70, 90));
    painter.with_transform(Affine::translate(28.0, 108.0), |painter| {
        painter.with_clip(
            Path::rectangle(Rect::new(0.0, 0.0, 340.0, 220.0)),
            |painter| left.paint(painter, Color::WHITE),
        );
    });
    painter.fill_rect(
        Rect::new(412.0, 108.0, 200.0, 220.0),
        Color::rgb(12, 30, 46),
    );
    painter.with_transform(Affine::translate(432.0, 128.0), |painter| {
        right.paint(painter, Color::rgb(255, 192, 83))
    });
    Ok(scene)
}

/// Selection/caret overlays use the same retained layout as glyph rendering.
pub fn text_selection_preview() -> Result<Scene, Box<dyn std::error::Error>> {
    use zui_api::text::{TextAffinity, TextPosition};
    let (mut service, families) = crate::preview_support::text()?;
    let text = "ZUI · 文字选择与光标\n中文与 Latin 共用同一份排版结果，换行后仍能准确绘制选择区域。\n组合字符 e\u{301} 不会被光标拆开。";
    let result = service.layout_interactive(TextRequest {
        text,
        style: &TextStyle {
            families,
            font_size: 26.0,
            line_height: 1.45,
            align: TextAlign::Start,
        },
        width: TextWidth::AtMost(560.0),
    })?;
    let start = text.find("中文").unwrap();
    let end = text.find("组合").unwrap();
    let position = |byte_index| TextPosition {
        byte_index,
        affinity: TextAffinity::Downstream,
    };
    let selected = result
        .geometry
        .selection_rects(position(start), position(end))?;
    let caret = result.geometry.caret(position(end), 2.0)?;
    let mut scene = Scene::new();
    scene
        .painter()
        .with_transform(Affine::translate(32.0, 48.0), |painter| {
            for rect in selected {
                painter.fill_rect(rect, Color::rgb(18, 89, 113));
            }
            result.layout.paint(painter, Color::rgb(225, 241, 246));
            painter.fill_rect(caret, Color::rgb(0, 220, 240));
        });
    Ok(scene)
}
