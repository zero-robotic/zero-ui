use zui_core::Color;

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
    pub spacing: f32,
    pub text_size: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::WHITE,
            foreground: Color::BLACK,
            accent: Color {
                r: 0.2,
                g: 0.4,
                b: 0.9,
                a: 1.0,
            },
            spacing: 8.0,
            text_size: 14.0,
        }
    }
}
