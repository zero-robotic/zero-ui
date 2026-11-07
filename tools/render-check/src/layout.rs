//! Public containers -> persistent runtime -> injected layout -> Scene -> either renderer.
use zui_api::layout::*;
use zui_core::{Tree, TreeError, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::Scene;
use zui_types::{Color, Size};
use zui_widgets::{Container, grid, row};

pub fn layout_preview(viewport: Size) -> Result<Scene, TreeError> {
    let sidebar = Container::empty()
        .background(Color::rgb(25, 50, 80))
        .keyed("sidebar")
        .style(|style| {
            style.width = Dimension::Length(144.0);
            style.shrink = 0.0;
        })
        .build();
    let cards = [
        Color::rgb(41, 210, 230),
        Color::rgb(255, 192, 83),
        Color::rgb(95, 100, 220),
        Color::rgb(45, 170, 135),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, color)| {
        Container::empty()
            .background(color)
            .keyed(index as u64)
            .build()
    });
    let grid = grid([Track::Fraction(1.0); 2], cards)
        .keyed("grid")
        .style(|style| {
            style.grow = 1.0;
            style.min_width = Dimension::Length(0.0);
            style.gap = Size::new(12.0, 12.0);
            style.rows = vec![Track::Fraction(1.0); 2];
        })
        .build();
    let view = row([sidebar, grid])
        .style(|style| {
            style.width = Dimension::Percent(1.0);
            style.height = Dimension::Percent(1.0);
            style.padding = Edges::all(16.0);
            style.gap = Size::new(16.0, 0.0);
        })
        .build();
    let mut tree = Tree::from_view(view, TreeLimits::default())?;
    tree.layout(&mut TaffyLayout::default(), viewport)?;
    tree.scene()
}
