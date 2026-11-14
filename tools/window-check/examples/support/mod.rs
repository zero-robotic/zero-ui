use std::time::Duration;
use zui_api::{
    layout::{Dimension, Track},
    view::View,
};
use zui_app::{ViewAction, ViewApplication};
use zui_platform::{InputEvent, WindowEvent, WindowMetrics};
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Point};
use zui_widgets::{TextView, VirtualListView, column, grid, stack};

pub(crate) struct Model {
    pub(crate) kind: &'static str,
    pub(crate) value: u64,
    pub(crate) families: Vec<String>,
}
impl ViewApplication for Model {
    type Message = ();
    fn view(&self) -> View {
        let theme = Theme::dark();
        let labels = (0..120).map(|i| {
            let value = if i == 0 && matches!(self.kind, "local_text" | "burst32") {
                self.value
            } else {
                i
            };
            let mut label = TextView::themed(
                &theme,
                format!("T{i:03}: {value:05}"),
                self.families.clone(),
                TypeRole::Caption,
            );
            if (i == 0 && self.kind == "color") || self.kind == "full_damage" {
                label.color = if self.value.is_multiple_of(2) {
                    Color::WHITE
                } else {
                    Color::rgb(0, 220, 240)
                };
            }
            View::new(label)
                .keyed(i)
                .styled(zui_api::layout::LayoutStyle {
                    width: Dimension::Length(100.0),
                    height: Dimension::Length(28.0),
                    ..Default::default()
                })
        });
        let body = grid([Track::Length(100.0); 8], labels)
            .keyed("grid")
            .build();
        let mut children = vec![body];
        if self.kind == "popup" && !self.value.is_multiple_of(2) {
            children.push(
                column([View::new(TextView::themed(
                    &theme,
                    "Popup",
                    self.families.clone(),
                    TypeRole::Body,
                ))])
                .keyed("popup")
                .background(Color::rgb(60, 60, 60))
                .style(|s| {
                    s.width = Dimension::Length(240.0);
                    s.height = Dimension::Length(80.0);
                })
                .build(),
            );
        }
        if self.kind == "virtual_idle" {
            let families = self.families.clone();
            children = vec![
                VirtualListView::new(10_000, 28.0, move |i| {
                    View::new(TextView::themed(
                        &theme,
                        format!("Row {i}"),
                        families.clone(),
                        TypeRole::Caption,
                    ))
                    .keyed(i as u64)
                })
                .style(|s| {
                    s.width = Dimension::Length(300.0);
                    s.height = Dimension::Length(420.0);
                })
                .build(),
            ];
        }
        stack(children)
            .style(|s| {
                s.width = Dimension::Length(800.0);
                s.height = Dimension::Length(480.0);
            })
            .build()
    }
    fn update(&mut self, e: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if e == WindowEvent::Wake && !matches!(self.kind, "virtual_idle" | "pointer32") {
            self.value += 1;
            ViewAction::rebuild()
        } else {
            ViewAction::default()
        }
    }
}
pub(crate) fn percentile(v: &mut [f64], q: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q) as usize]
}

pub(crate) fn events(kind: &str) -> Vec<WindowEvent> {
    if kind == "pointer32" {
        (0..32)
            .map(|i| {
                WindowEvent::Input(InputEvent::PointerMoved(Point::new(i as f64 * 5.0, 470.0)))
            })
            .collect()
    } else {
        vec![WindowEvent::Wake; if kind == "burst32" { 32 } else { 1 }]
    }
}
