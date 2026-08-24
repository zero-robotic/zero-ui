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
pub struct TextInputStyle {
    pub width: Dip,
    pub height: Dip,
    pub padding_x: Dip,
    pub background: Color,
    pub focused_background: Color,
    pub foreground: Color,
    pub caret_color: Color,
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
struct TextInputConfig {
    width: Option<f32>,
    height: Option<f32>,
    padding_x: Option<f32>,
    background: Option<String>,
    focused_background: Option<String>,
    foreground: Option<String>,
    caret_color: Option<String>,
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
            if let Some(value) = config.font_size {
                theme.text_input.font_size = value;
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
