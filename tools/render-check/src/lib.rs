//! Top-level rendering fixtures. Lower-level crates never depend on this crate.
mod panel;
mod preview_support;
pub use panel::panel_preview;
mod metric;
pub use metric::metric_preview;
mod sparkline;
pub use sparkline::sparkline_preview;
mod accordion;
pub use accordion::accordion_preview;
mod button;
mod checkbox;
mod progress;
mod radio;
mod tabs;
pub use tabs::tabs_preview;
mod segmented;
pub use segmented::segmented_preview;
mod scroll;
mod slider;
mod switch;
pub use switch::switch_preview;
mod stack;
pub use checkbox::checkbox_preview;
pub use progress::progress_preview;
pub use radio::{radio_horizontal_preview, radio_preview};
pub use scroll::{scroll_both_preview, scroll_preview};
pub use slider::slider_preview;
pub use stack::stack_preview;
mod layout;
pub use button::button_preview;
mod text;
mod text_input;
pub use text_input::text_input_preview;
mod theme;
pub use layout::layout_preview;
pub use text::{text_preview, text_selection_preview, text_widget_preview};
pub use theme::{desktop_preview, dialog_preview, theme_preview, theme_view};
use zui_paint::{Path, Scene};
use zui_types::{Affine, Color, Point, Rect};

pub fn preview() -> Scene {
    let mut scene = Scene::new();
    let mut painter = scene.painter();
    let cyan = Color::rgb(41, 210, 230);
    for column in 0..16 {
        painter.fill_rect(
            Rect::new(column as f64 * 40.0, 0.0, 1.0, 360.0),
            Color::rgb(15, 35, 54),
        );
    }
    for row in 0..9 {
        painter.fill_rect(
            Rect::new(0.0, row as f64 * 40.0, 640.0, 1.0),
            Color::rgb(15, 35, 54),
        );
    }
    painter.with_transform(Affine::translate(32.0, 32.0), |painter| {
        let mut panel = Path::new();
        for (index, (x, y)) in [
            (0., 16.),
            (16., 0.),
            (560., 0.),
            (576., 16.),
            (576., 280.),
            (560., 296.),
            (16., 296.),
            (0., 280.),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 0 {
                panel.move_to(Point::new(x, y));
            } else {
                panel.line_to(Point::new(x, y));
            }
        }
        panel.close();
        painter.fill(panel.clone(), Color::rgba(10, 27, 44, 240));
        painter.stroke(panel, cyan, 1.5);
        painter.with_clip(
            Path::rectangle(Rect::new(24.0, 24.0, 528.0, 248.0)),
            |painter| {
                for (index, height) in [56., 92., 72., 126., 160., 140., 198., 178.]
                    .into_iter()
                    .enumerate()
                {
                    let x = 42.0 + index as f64 * 60.0;
                    painter.fill_rect(
                        Rect::new(x, 256.0 - height, 30.0, height),
                        Color::rgba(41, 210, 230, 110),
                    );
                    painter.fill_rect(Rect::new(x, 256.0 - height, 30.0, 3.0), cyan);
                }
                let mut curve = Path::new();
                curve.move_to(Point::new(-20.0, 220.0));
                curve.cubic_to(
                    Point::new(160.0, 260.0),
                    Point::new(200.0, -10.0),
                    Point::new(330.0, 112.0),
                );
                curve.quad_to(Point::new(420.0, 180.0), Point::new(600.0, 0.0));
                painter.stroke(curve, Color::rgb(255, 192, 83), 3.0);
            },
        );
    });
    scene
}

pub fn regression_scene() -> Scene {
    let mut scene = Scene::new();
    let mut painter = scene.painter();
    painter.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), Color::rgba(255, 0, 0, 128));
    painter.with_transform(Affine::translate(12.0, 4.0), |painter| {
        painter.with_clip(Path::rectangle(Rect::new(0.0, 0.0, 8.0, 8.0)), |painter| {
            painter.fill_rect(Rect::new(-8.0, -8.0, 24.0, 24.0), Color::rgb(0, 255, 0));
            painter.with_clip(Path::rectangle(Rect::new(2.0, 2.0, 4.0, 4.0)), |painter| {
                painter.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), Color::rgb(0, 0, 255));
            });
            painter.fill_rect(Rect::new(0.0, 7.0, 8.0, 1.0), Color::WHITE);
        });
    });
    painter.fill_rect(Rect::new(24.0, 24.0, 4.0, 4.0), Color::WHITE);
    scene
}

mod virtual_list;
pub use virtual_list::{virtual_list_measured_preview, virtual_list_preview};
