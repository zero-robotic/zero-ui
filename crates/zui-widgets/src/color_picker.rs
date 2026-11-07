use crate::{
    ButtonView, CanvasView, TextInputView, TextView, action_button::ActionButtonView, column,
    commit_input::CommitInputView, row,
};
use alloc::{rc::Rc, string::String, vec::Vec};
use zui_api::{
    action::ActionId,
    layout::{Dimension, LayoutStyle},
    view::View,
};
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorChanged {
    pub color: Color,
}
pub fn parse_hex_color(text: &str) -> Option<Color> {
    let text = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !matches!(text.len(), 6 | 8) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i| u8::from_str_radix(&text[i..i + 2], 16).ok();
    Some(Color::rgba(
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if text.len() == 8 { byte(6)? } else { 255 },
    ))
}
pub struct ColorPickerView {
    pub color: Color,
    /// Retain HsvChanged.hsv here to preserve hue when saturation/value reach zero.
    pub hsv: Option<crate::HsvColor>,
    pub disabled: bool,
    pub presets: Vec<Color>,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
}
impl ColorPickerView {
    pub fn themed(theme: &Theme, color: Color, families: Vec<String>, action: ActionId) -> Self {
        Self {
            color,
            hsv: None,
            disabled: false,
            presets: alloc::vec![
                Color::rgb(32, 200, 220),
                Color::rgb(60, 130, 240),
                Color::rgb(80, 180, 110),
                Color::rgb(240, 180, 60),
                Color::rgb(220, 70, 80),
                Color::WHITE,
                Color::BLACK
            ],
            theme: theme.clone(),
            families,
            action,
        }
    }
    pub fn build(&self) -> View {
        let selected = self.color;
        let preview = View::new(CanvasView::new(Size::new(180.0, 40.0), move |c, p| {
            let b = c.content_bounds;
            let cols = ((b.size.width / 10.0) as usize).saturating_add(1).min(256);
            let rows = ((b.size.height / 10.0) as usize).saturating_add(1).min(256);
            for y in 0..rows {
                for x in 0..cols {
                    p.fill_rect(
                        Rect::new(
                            b.origin.x + x as f64 * 10.0,
                            b.origin.y + y as f64 * 10.0,
                            10.0,
                            10.0,
                        ),
                        if (x + y) % 2 == 0 {
                            Color::WHITE
                        } else {
                            Color::rgb(170, 170, 170)
                        },
                    );
                }
            }
            p.fill_rect(b, selected);
        }))
        .styled(LayoutStyle {
            width: Dimension::Percent(1.0),
            height: Dimension::Length(40.0),
            ..Default::default()
        });
        let mut input = TextInputView::themed(
            &self.theme,
            alloc::format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                selected.r,
                selected.g,
                selected.b,
                selected.a
            ),
            self.families.clone(),
            self.action,
        );
        input.disabled = self.disabled;
        let hex = View::new(CommitInputView {
            cancel: None,
            reset_on_blur: true,
            input,
            parse: Rc::new(|text| parse_hex_color(text).map(|color| ColorChanged { color })),
        })
        .keyed("hex");
        let channels = [
            ("R", selected.r),
            ("G", selected.g),
            ("B", selected.b),
            ("A", selected.a),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let mut input = TextInputView::themed(
                &self.theme,
                alloc::format!("{value}"),
                self.families.clone(),
                self.action,
            );
            input.disabled = self.disabled;
            input.style.natural_width = 60.0;
            let editor = View::new(CommitInputView {
                cancel: None,
                reset_on_blur: true,
                input,
                parse: Rc::new(move |text| {
                    let value = text.trim().parse::<u8>().ok()?;
                    let mut color = selected;
                    match index {
                        0 => color.r = value,
                        1 => color.g = value,
                        2 => color.b = value,
                        _ => color.a = value,
                    };
                    Some(ColorChanged { color })
                }),
            });
            column([
                View::new(TextView::themed(
                    &self.theme,
                    label,
                    self.families.clone(),
                    TypeRole::Caption,
                )),
                editor,
            ])
            .keyed(index as u64)
            .style(|s| s.grow = 1.0)
            .build()
        });
        let presets = self.presets.iter().enumerate().map(|(index, &color)| {
            let mut button =
                ButtonView::themed(&self.theme, " ", self.families.clone(), self.action);
            button.disabled = self.disabled;
            button.style.normal.background = color;
            button.style.hovered.background = color;
            button.style.pressed.background = color;
            View::new(ActionButtonView {
                button,
                payload: ColorChanged { color },
            })
            .keyed(index as u64)
            .styled(LayoutStyle {
                width: Dimension::Length(32.0),
                height: Dimension::Length(28.0),
                ..Default::default()
            })
        });
        column([
            preview,
            hex,
            View::new(crate::HsvPickerView {
                hsv: self
                    .hsv
                    .unwrap_or_else(|| crate::HsvColor::from_color(self.color)),
                alpha: self.color.a,
                disabled: self.disabled,
                action: self.action,
            })
            .styled(LayoutStyle {
                height: Dimension::Length(156.0),
                ..Default::default()
            }),
            row(channels)
                .style(|s| {
                    s.wrap = true;
                    s.gap = Size::new(6.0, 6.0);
                })
                .build(),
            row(presets)
                .style(|s| {
                    s.wrap = true;
                    s.gap = Size::new(6.0, 6.0);
                })
                .build(),
        ])
        .style(|s| s.gap = Size::new(0.0, 8.0))
        .build()
    }
}
