#![cfg(feature = "cpu")]
use std::sync::Arc;
use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges},
};
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Size};
use zui_widgets::{
    AccordionExpansion as Expansion, AccordionId as Id, AccordionItem, AccordionView, Container,
};

#[test]
fn collapse_removes_content_pixels_and_reflows_following_headers_at_both_scales() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let colors = [
        Color::rgb(231, 33, 44),
        Color::rgb(32, 224, 45),
        Color::rgb(31, 44, 233),
    ];
    let description = |expansion| {
        let mut items: Vec<_> = (1..=3)
            .map(|id| AccordionItem::new(Id(id), format!("参数 {id}")))
            .collect();
        items[2].disabled = true;
        let mut group = AccordionView::themed(
            &Theme::dark(),
            items,
            families.clone(),
            expansion,
            ActionId(1),
        );
        group.layout.width = Dimension::Length(160.0);
        group.layout.gap = Size::new(0.0, 5.0);
        group.header_layout.height = Dimension::Length(30.0);
        group.header_layout.padding = Edges::all(0.0);
        group
            .build(|id| {
                Container::empty()
                    .background(colors[(id.0 - 1) as usize])
                    .style(|s| s.height = Dimension::Length(20.0))
                    .build()
            })
            .unwrap()
    };
    let mut tree =
        zui_core::Tree::from_view(description(Expansion::Single(None)), Default::default())
            .unwrap();
    let mut renderer = zui_render_cpu::CpuRenderer::default();
    for (expansion, visible, sample_y) in [
        (
            Expansion::Multiple([Id(1), Id(3)].into()),
            [true, false, true],
            [40, 0, 120],
        ),
        (
            Expansion::Single(Some(Id(2))),
            [false, true, false],
            [0, 70, 0],
        ),
        (Expansion::Single(None), [false; 3], [0; 3]),
    ] {
        tree.reconcile(description(expansion)).unwrap();
        tree.layout_with_text(
            &mut zui_layout::TaffyLayout::default(),
            &mut text,
            Size::new(160.0, 200.0),
        )
        .unwrap();
        let scene = tree.scene().unwrap();
        for scale in [1u32, 2] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new(160 * scale, 200 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            for (index, color) in colors.iter().enumerate() {
                let rgba = [color.r, color.g, color.b, 255];
                let count = pixels
                    .pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| **pixel == rgba)
                    .count();
                assert_eq!(
                    count,
                    if visible[index] {
                        (160 * 20 * scale * scale) as usize
                    } else {
                        0
                    }
                );
                if visible[index] {
                    let offset =
                        ((sample_y[index] * scale * 160 * scale + 10 * scale) * 4) as usize;
                    assert_eq!(&pixels.pixels()[offset..offset + 4], &rgba);
                }
            }
        }
    }
}
