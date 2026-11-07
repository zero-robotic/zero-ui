//! Semantic values only: no widgets, runtime, renderer, fonts or global state.
#![no_std]
use core::fmt;
use zui_types::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ColorRole {
    Background,
    Surface,
    SurfaceRaised,
    Text,
    TextMuted,
    Accent,
    OnAccent,
    Border,
    Success,
    Warning,
    Danger,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum TypeRole {
    Caption,
    Body,
    Heading,
    Display,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Space {
    None,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    font_size: f32,
    line_height: f32,
}
impl Typography {
    pub fn new(font_size: f32, line_height: f32) -> Result<Self, ThemeError> {
        if !font_size.is_finite()
            || font_size <= 0.0
            || !line_height.is_finite()
            || line_height <= 0.0
        {
            return Err(ThemeError::InvalidTypography);
        }
        Ok(Self {
            font_size,
            line_height,
        })
    }
    pub const fn font_size(self) -> f32 {
        self.font_size
    }
    /// Multiplier of font size, not an absolute length.
    pub const fn line_height(self) -> f32 {
        self.line_height
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeError {
    InvalidTypography,
    InvalidSpacing,
}
impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "theme error: {self:?}")
    }
}
impl core::error::Error for ThemeError {}

/// Device-independent control metrics shared by CPU and GPU rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlMetrics {
    pub radius: u16,
    pub panel_radius: u16,
    pub min_height: u16,
    pub padding_x: u16,
}

/// Owned values can be cloned for local overrides. Resolve tokens while building
/// a View; rebuilding that View applies theme changes through normal reconciliation.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    controls: ControlMetrics,
    colors: [Color; 11],
    typography: [Typography; 4],
    spacing: [f32; 6],
}
impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
impl Theme {
    fn palette(colors: [Color; 11]) -> Self {
        Self {
            controls: ControlMetrics::default(),
            colors,
            typography: [
                Typography {
                    font_size: 14.0,
                    line_height: 1.4,
                },
                Typography {
                    font_size: 18.0,
                    line_height: 1.4,
                },
                Typography {
                    font_size: 26.0,
                    line_height: 1.3,
                },
                Typography {
                    font_size: 36.0,
                    line_height: 1.2,
                },
            ],
            spacing: [0.0, 4.0, 8.0, 16.0, 24.0, 32.0],
        }
    }
    pub fn dark() -> Self {
        Self::palette([
            Color::rgb(18, 22, 28),
            Color::rgb(30, 36, 44),
            Color::rgb(42, 50, 60),
            Color::rgb(238, 242, 248),
            Color::rgb(173, 185, 201),
            Color::rgb(94, 175, 255),
            Color::rgb(10, 24, 38),
            Color::rgb(74, 88, 105),
            Color::rgb(85, 207, 150),
            Color::rgb(255, 197, 88),
            Color::rgb(255, 114, 125),
        ])
    }
    pub fn light() -> Self {
        Self::palette([
            Color::rgb(242, 245, 249),
            Color::WHITE,
            Color::rgb(231, 237, 244),
            Color::rgb(24, 34, 48),
            Color::rgb(78, 94, 113),
            Color::rgb(0, 92, 184),
            Color::WHITE,
            Color::rgb(161, 175, 193),
            Color::rgb(15, 112, 68),
            Color::rgb(135, 82, 0),
            Color::rgb(186, 35, 54),
        ])
    }
    /// A technical-dashboard palette; does not imply special rendering effects.
    pub fn dashboard() -> Self {
        Self::palette([
            Color::rgb(5, 14, 25),
            Color::rgb(12, 30, 46),
            Color::rgb(20, 43, 62),
            Color::rgb(225, 245, 250),
            Color::rgb(147, 186, 201),
            Color::rgb(41, 210, 230),
            Color::rgb(5, 25, 35),
            Color::rgb(35, 91, 116),
            Color::rgb(63, 215, 156),
            Color::rgb(255, 192, 83),
            Color::rgb(255, 105, 126),
        ])
    }
    /// Compact desktop presentation. Legacy themes retain their original metrics.
    pub fn desktop_dark() -> Self {
        Self::desktop(false)
    }
    pub fn desktop_light() -> Self {
        Self::desktop(true)
    }
    fn desktop(light: bool) -> Self {
        let mut theme = if light { Self::light() } else { Self::dark() };
        theme.controls = ControlMetrics {
            radius: 6,
            panel_radius: 8,
            min_height: 32,
            padding_x: 10,
        };
        theme.typography = [
            Typography {
                font_size: 12.0,
                line_height: 1.5,
            },
            Typography {
                font_size: 14.0,
                line_height: 1.4,
            },
            Typography {
                font_size: 20.0,
                line_height: 1.3,
            },
            Typography {
                font_size: 28.0,
                line_height: 1.2,
            },
        ];
        theme.spacing = [0.0, 4.0, 8.0, 12.0, 16.0, 24.0];
        theme.set_color(
            ColorRole::Border,
            if light {
                Color::rgb(213, 219, 227)
            } else {
                Color::rgb(54, 63, 76)
            },
        );
        theme.set_color(
            ColorRole::Accent,
            if light {
                Color::rgb(49, 104, 210)
            } else {
                Color::rgb(124, 164, 238)
            },
        );
        theme
    }
    pub fn controls(&self) -> ControlMetrics {
        self.controls
    }
    pub fn set_controls(&mut self, metrics: ControlMetrics) {
        self.controls = metrics;
    }
    /// Opaque, subdued selection surface; never exposes underlying content.
    pub fn selection_background(&self) -> Color {
        if self.controls.min_height == 0 {
            return self.color(ColorRole::Accent);
        }
        let base = self.color(ColorRole::Surface);
        let accent = self.color(ColorRole::Accent);
        let blend = |a: u8, b: u8| ((u16::from(a) * 4 + u16::from(b)) / 5) as u8;
        Color::rgb(
            blend(base.r, accent.r),
            blend(base.g, accent.g),
            blend(base.b, accent.b),
        )
    }
    pub fn selection_foreground(&self) -> Color {
        self.color(if self.controls.min_height == 0 {
            ColorRole::OnAccent
        } else {
            ColorRole::Text
        })
    }
    pub fn color(&self, role: ColorRole) -> Color {
        self.colors[role as usize]
    }
    pub fn typography(&self, role: TypeRole) -> Typography {
        self.typography[role as usize]
    }
    pub fn space(&self, role: Space) -> f32 {
        self.spacing[role as usize]
    }
    pub fn set_color(&mut self, role: ColorRole, value: Color) {
        self.colors[role as usize] = value;
    }
    pub fn set_typography(&mut self, role: TypeRole, value: Typography) {
        self.typography[role as usize] = value;
    }
    pub fn set_space(&mut self, role: Space, value: f32) -> Result<(), ThemeError> {
        if !value.is_finite() || value < 0.0 {
            return Err(ThemeError::InvalidSpacing);
        }
        self.spacing[role as usize] = value;
        Ok(())
    }
}
