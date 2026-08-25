use std::{fs, path::Path};

use serde::Deserialize;
use zui_core::{Color, Dip};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeToken {
    Background,
    Foreground,
    Accent,
    Spacing,
    TextSize,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub spacing: Dip,
    pub text: TextStyle,
    pub button: ButtonStyle,
    pub checkbox: CheckboxStyle,
    pub switch: SwitchStyle,
    pub icon: IconStyle,
    pub icon_button: IconButtonStyle,
    pub text_input: TextInputStyle,
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub color: Color,
    pub font_size: u32,
}

#[derive(Clone, Debug)]
pub struct ButtonStyle {
    pub height: Dip,
    pub padding_x: Dip,
    pub radius: Dip,
    pub background: Color,
    pub hover_background: Color,
    pub foreground: Color,
    pub font_size: u32,
}

#[derive(Clone, Debug)]
pub struct CheckboxStyle {
    pub size: Dip,
    pub gap: Dip,
    pub radius: Dip,
    pub background: Color,
    pub checked_background: Color,
    pub foreground: Color,
    pub font_size: u32,
}

#[derive(Clone, Debug)]
pub struct SwitchStyle {
    pub width: Dip,
    pub height: Dip,
    pub knob_size: Dip,
    pub background: Color,
    pub checked_background: Color,
    pub knob: Color,
    pub gap: Dip,
    pub font_size: u32,
}

#[derive(Clone, Debug)]
pub struct IconStyle {
    pub size: Dip,
    pub color: Color,
    pub stroke_width: Dip,
}

#[derive(Clone, Debug)]
pub struct IconButtonStyle {
    pub size: Dip,
    pub radius: Dip,
    pub background: Color,
    pub hover_background: Color,
    pub foreground: Color,
    pub icon_size: Dip,
    pub stroke_width: Dip,
}

#[derive(Clone, Debug)]
pub struct TextInputStyle {
    pub width: Dip,
    pub height: Dip,
    pub padding_x: Dip,
    pub background: Color,
    pub focused_background: Color,
    pub foreground: Color,
    pub caret_color: Color,
    pub selection_background: Color,
    pub font_size: u32,
}

impl Default for Theme {
    fn default() -> Self {
        let accent = Color {
            r: 0.2,
            g: 0.4,
            b: 0.9,
            a: 1.0,
        };
        Self {
            background: Color {
                r: 0.08,
                g: 0.10,
                b: 0.14,
                a: 1.0,
            },
            foreground: Color::WHITE,
            accent,
            spacing: Dip(8.0),
            text: TextStyle {
                color: Color::WHITE,
                font_size: 3,
            },
            button: ButtonStyle {
                height: Dip(40.0),
                padding_x: Dip(16.0),
                radius: Dip(8.0),
                background: accent,
                hover_background: Color {
                    r: 0.14,
                    g: 0.34,
                    b: 0.72,
                    a: 1.0,
                },
                foreground: Color::WHITE,
                font_size: 3,
            },
            checkbox: CheckboxStyle {
                size: Dip(22.0),
                gap: Dip(8.0),
                radius: Dip(4.0),
                background: Color {
                    r: 0.82,
                    g: 0.85,
                    b: 0.9,
                    a: 1.0,
                },
                checked_background: accent,
                foreground: Color::WHITE,
                font_size: 2,
            },
            switch: SwitchStyle {
                width: Dip(42.0),
                height: Dip(24.0),
                knob_size: Dip(18.0),
                background: Color {
                    r: 0.45,
                    g: 0.48,
                    b: 0.55,
                    a: 1.0,
                },
                checked_background: accent,
                knob: Color::WHITE,
                gap: Dip(8.0),
                font_size: 3,
            },
            icon: IconStyle {
                size: Dip(24.0),
                color: Color::WHITE,
                stroke_width: Dip(2.0),
            },
            icon_button: IconButtonStyle {
                size: Dip(40.0),
                radius: Dip(8.0),
                background: Color {
                    r: 0.28,
                    g: 0.32,
                    b: 0.4,
                    a: 1.0,
                },
                hover_background: accent,
                foreground: Color::WHITE,
                icon_size: Dip(22.0),
                stroke_width: Dip(2.0),
            },
            text_input: TextInputStyle {
                width: Dip(280.0),
                height: Dip(40.0),
                padding_x: Dip(8.0),
                background: Color {
                    r: 0.82,
                    g: 0.85,
                    b: 0.9,
                    a: 1.0,
                },
                focused_background: Color {
                    r: 0.92,
                    g: 0.95,
                    b: 1.0,
                    a: 1.0,
                },
                foreground: Color::BLACK,
                caret_color: Color::BLACK,
                selection_background: Color {
                    r: 0.45,
                    g: 0.65,
                    b: 0.9,
                    a: 1.0,
                },
                font_size: 3,
            },
        }
    }
}

impl Theme {
    pub fn from_toml_str(source: &str) -> Result<Self, String> {
        let config: ThemeConfig = toml::from_str(source).map_err(|error| error.to_string())?;
        config.apply(Self::default())
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
        Self::from_toml_str(&source)
    }
}

#[derive(Default, Deserialize)]
struct ThemeConfig {
    background: Option<String>,
    foreground: Option<String>,
    accent: Option<String>,
    spacing: Option<f32>,
    text: Option<TextConfig>,
    button: Option<ButtonConfig>,
    checkbox: Option<CheckboxConfig>,
    switch: Option<SwitchConfig>,
    icon: Option<IconConfig>,
    icon_button: Option<IconButtonConfig>,
    text_input: Option<TextInputConfig>,
}

#[derive(Default, Deserialize)]
struct TextConfig {
    color: Option<String>,
    font_size: Option<u32>,
}

#[derive(Default, Deserialize)]
struct ButtonConfig {
    height: Option<f32>,
    padding_x: Option<f32>,
    radius: Option<f32>,
    background: Option<String>,
    hover_background: Option<String>,
    foreground: Option<String>,
    font_size: Option<u32>,
}

#[derive(Default, Deserialize)]
struct CheckboxConfig {
    size: Option<f32>,
    gap: Option<f32>,
    radius: Option<f32>,
    background: Option<String>,
    checked_background: Option<String>,
    foreground: Option<String>,
    font_size: Option<u32>,
}

#[derive(Default, Deserialize)]
struct SwitchConfig {
    width: Option<f32>,
    height: Option<f32>,
    knob_size: Option<f32>,
    background: Option<String>,
    checked_background: Option<String>,
    knob: Option<String>,
    gap: Option<f32>,
    font_size: Option<u32>,
}

#[derive(Default, Deserialize)]
struct IconConfig {
    size: Option<f32>,
    color: Option<String>,
    stroke_width: Option<f32>,
}

#[derive(Default, Deserialize)]
struct IconButtonConfig {
    size: Option<f32>,
    radius: Option<f32>,
    background: Option<String>,
    hover_background: Option<String>,
    foreground: Option<String>,
    icon_size: Option<f32>,
    stroke_width: Option<f32>,
}

#[derive(Default, Deserialize)]
struct TextInputConfig {
    width: Option<f32>,
    height: Option<f32>,
    padding_x: Option<f32>,
    background: Option<String>,
    focused_background: Option<String>,
    foreground: Option<String>,
    caret_color: Option<String>,
    selection_background: Option<String>,
    font_size: Option<u32>,
}

impl ThemeConfig {
    fn apply(self, mut theme: Theme) -> Result<Theme, String> {
        if let Some(value) = self.background {
            theme.background = parse_color(&value)?;
        }
        if let Some(value) = self.foreground {
            theme.foreground = parse_color(&value)?;
        }
        if let Some(value) = self.accent {
            theme.accent = parse_color(&value)?;
        }
        if let Some(value) = self.spacing {
            theme.spacing = Dip(value);
        }
        if let Some(config) = self.text {
            if let Some(value) = config.color {
                theme.text.color = parse_color(&value)?;
            }
            if let Some(value) = config.font_size {
                theme.text.font_size = value;
            }
        }
        if let Some(config) = self.button {
            if let Some(value) = config.height {
                theme.button.height = Dip(value);
            }
            if let Some(value) = config.padding_x {
                theme.button.padding_x = Dip(value);
            }
            if let Some(value) = config.radius {
                theme.button.radius = Dip(value);
            }
            if let Some(value) = config.background {
                theme.button.background = parse_color(&value)?;
            }
            if let Some(value) = config.hover_background {
                theme.button.hover_background = parse_color(&value)?;
            }
            if let Some(value) = config.foreground {
                theme.button.foreground = parse_color(&value)?;
            }
            if let Some(value) = config.font_size {
                theme.button.font_size = value;
            }
        }
        if let Some(config) = self.text_input {
            if let Some(value) = config.width {
                theme.text_input.width = Dip(value);
            }
            if let Some(value) = config.height {
                theme.text_input.height = Dip(value);
            }
            if let Some(value) = config.padding_x {
                theme.text_input.padding_x = Dip(value);
            }
            if let Some(value) = config.background {
                theme.text_input.background = parse_color(&value)?;
            }
            if let Some(value) = config.focused_background {
                theme.text_input.focused_background = parse_color(&value)?;
            }
            if let Some(value) = config.foreground {
                theme.text_input.foreground = parse_color(&value)?;
            }
            if let Some(value) = config.caret_color {
                theme.text_input.caret_color = parse_color(&value)?;
            }
            if let Some(value) = config.selection_background {
                theme.text_input.selection_background = parse_color(&value)?;
            }
            if let Some(value) = config.font_size {
                theme.text_input.font_size = value;
            }
        }
        if let Some(config) = self.checkbox {
            if let Some(value) = config.size {
                theme.checkbox.size = Dip(value);
            }
            if let Some(value) = config.gap {
                theme.checkbox.gap = Dip(value);
            }
            if let Some(value) = config.radius {
                theme.checkbox.radius = Dip(value);
            }
            if let Some(value) = config.background {
                theme.checkbox.background = parse_color(&value)?;
            }
            if let Some(value) = config.checked_background {
                theme.checkbox.checked_background = parse_color(&value)?;
            }
            if let Some(value) = config.foreground {
                theme.checkbox.foreground = parse_color(&value)?;
            }
            if let Some(value) = config.font_size {
                theme.checkbox.font_size = value;
            }
        }
        if let Some(config) = self.switch {
            if let Some(value) = config.width {
                theme.switch.width = Dip(value);
            }
            if let Some(value) = config.height {
                theme.switch.height = Dip(value);
            }
            if let Some(value) = config.knob_size {
                theme.switch.knob_size = Dip(value);
            }
            if let Some(value) = config.background {
                theme.switch.background = parse_color(&value)?;
            }
            if let Some(value) = config.checked_background {
                theme.switch.checked_background = parse_color(&value)?;
            }
            if let Some(value) = config.knob {
                theme.switch.knob = parse_color(&value)?;
            }
            if let Some(value) = config.gap {
                theme.switch.gap = Dip(value);
            }
            if let Some(value) = config.font_size {
                theme.switch.font_size = value;
            }
        }
        if let Some(config) = self.icon {
            if let Some(value) = config.size {
                theme.icon.size = Dip(value);
            }
            if let Some(value) = config.color {
                theme.icon.color = parse_color(&value)?;
            }
            if let Some(value) = config.stroke_width {
                theme.icon.stroke_width = Dip(value);
            }
        }
        if let Some(config) = self.icon_button {
            if let Some(value) = config.size {
                theme.icon_button.size = Dip(value);
            }
            if let Some(value) = config.radius {
                theme.icon_button.radius = Dip(value);
            }
            if let Some(value) = config.background {
                theme.icon_button.background = parse_color(&value)?;
            }
            if let Some(value) = config.hover_background {
                theme.icon_button.hover_background = parse_color(&value)?;
            }
            if let Some(value) = config.foreground {
                theme.icon_button.foreground = parse_color(&value)?;
            }
            if let Some(value) = config.icon_size {
                theme.icon_button.icon_size = Dip(value);
            }
            if let Some(value) = config.stroke_width {
                theme.icon_button.stroke_width = Dip(value);
            }
        }
        Ok(theme)
    }
}

fn parse_color(value: &str) -> Result<Color, String> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 && hex.len() != 8 {
        return Err(format!(
            "invalid color '{value}', expected #RRGGBB or #RRGGBBAA"
        ));
    }
    let parse =
        |part: &str| u8::from_str_radix(part, 16).map_err(|_| format!("invalid color '{value}'"));
    let r = parse(&hex[0..2])?;
    let g = parse(&hex[2..4])?;
    let b = parse(&hex[4..6])?;
    let a = if hex.len() == 8 {
        parse(&hex[6..8])?
    } else {
        255
    };
    Ok(Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    })
}
