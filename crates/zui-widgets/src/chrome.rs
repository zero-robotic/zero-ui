//! Small compositional surfaces; no application actions or platform dependencies.
use crate::{Container, TextView, column, row};
use alloc::{string::String, vec::Vec};
use zui_api::{
    layout::{AvailableSpace, Dimension, Edges, LayoutError, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Color, Point, Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeparatorAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeparatorView {
    pub axis: SeparatorAxis,
    pub color: Color,
    pub thickness: f32,
}
impl SeparatorView {
    pub fn themed(theme: &Theme, axis: SeparatorAxis) -> Self {
        Self {
            axis,
            color: theme.color(ColorRole::Border),
            thickness: 1.0,
        }
    }
}
pub struct Separator(SeparatorView);
impl ViewSpec for SeparatorView {
    type Widget = Separator;
    fn build(&self) -> Separator {
        Separator(*self)
    }
    fn update(&self, w: &mut Separator) -> Invalidation {
        let result = if self.axis != w.0.axis || self.thickness != w.0.thickness {
            Invalidation::Layout
        } else if self.color != w.0.color {
            Invalidation::Paint
        } else {
            Invalidation::None
        };
        w.0 = *self;
        result
    }
}
impl Widget for Separator {
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.prepare(PrepareContext {
            id: c.id,
            layout: Default::default(),
            content_size: Size::ZERO,
            text: c.text,
        })?;
        let length = |space| match space {
            AvailableSpace::Definite(v) => v as f64,
            _ => 24.0,
        };
        Ok(match self.0.axis {
            SeparatorAxis::Horizontal => Size::new(
                c.input
                    .known_width
                    .map(f64::from)
                    .unwrap_or_else(|| length(c.input.available_width)),
                c.input
                    .known_height
                    .map(f64::from)
                    .unwrap_or(self.0.thickness as f64),
            ),
            SeparatorAxis::Vertical => Size::new(
                c.input
                    .known_width
                    .map(f64::from)
                    .unwrap_or(self.0.thickness as f64),
                c.input
                    .known_height
                    .map(f64::from)
                    .unwrap_or_else(|| length(c.input.available_height)),
            ),
        })
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        if self.0.thickness.is_finite() && self.0.thickness > 0.0 {
            Ok(())
        } else {
            Err(LayoutError::InvalidStyle(c.id))
        }
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        let b = c.content_bounds;
        let r = match self.0.axis {
            SeparatorAxis::Horizontal => {
                let h = (self.0.thickness as f64).min(b.size.height);
                Rect::new(
                    b.origin.x,
                    b.origin.y + (b.size.height - h) / 2.0,
                    b.size.width,
                    h,
                )
            }
            SeparatorAxis::Vertical => {
                let w = (self.0.thickness as f64).min(b.size.width);
                Rect::new(
                    b.origin.x + (b.size.width - w) / 2.0,
                    b.origin.y,
                    w,
                    b.size.height,
                )
            }
        };
        p.fill_rect(r, self.0.color);
    }
}
pub struct GroupBoxView {
    pub title: String,
    pub theme: Theme,
    pub families: Vec<String>,
}
impl GroupBoxView {
    pub fn themed(theme: &Theme, title: impl Into<String>, families: Vec<String>) -> Self {
        Self {
            title: title.into(),
            theme: theme.clone(),
            families,
        }
    }
    pub fn build(self, content: View) -> View {
        let body = column([
            View::new(TextView::themed(
                &self.theme,
                self.title,
                self.families,
                TypeRole::Body,
            )),
            content,
        ])
        .style(|s| {
            s.padding = Edges::all(12.0);
            s.gap = Size::new(0.0, 10.0);
            s.min_width = Dimension::Length(0.0);
        })
        .build();
        View::new(GroupFrame {
            border: self.theme.color(ColorRole::Border),
            background: self.theme.color(ColorRole::Surface),
        })
        .styled(LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        })
        .with_children([body])
    }
}
#[derive(Clone, Copy, PartialEq)]
struct GroupFrame {
    border: Color,
    background: Color,
}
struct Frame(GroupFrame);
impl ViewSpec for GroupFrame {
    type Widget = Frame;
    fn build(&self) -> Frame {
        Frame(*self)
    }
    fn update(&self, w: &mut Frame) -> Invalidation {
        let changed = *self != w.0;
        w.0 = *self;
        if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}
impl Widget for Frame {
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        p.fill_rect(
            Rect::new(0.0, 0.0, c.size.width, c.size.height),
            self.0.background,
        );
        if c.size.width > 1.0 && c.size.height > 1.0 {
            p.stroke(
                Path::rectangle(Rect::new(0.5, 0.5, c.size.width - 1.0, c.size.height - 1.0)),
                self.0.border,
                1.0,
            );
        }
    }
}
pub struct ToolbarView {
    pub theme: Theme,
    pub wrap: bool,
}
impl ToolbarView {
    pub fn themed(theme: &Theme) -> Self {
        Self {
            theme: theme.clone(),
            wrap: true,
        }
    }
    pub fn build(self, items: impl IntoIterator<Item = View>) -> View {
        row(items)
            .background(self.theme.color(ColorRole::SurfaceRaised))
            .style(|s| {
                s.wrap = self.wrap;
                s.gap = Size::new(8.0, 8.0);
                s.padding = Edges::all(8.0);
            })
            .build()
    }
}
pub type ToolBarView = ToolbarView;
pub struct StatusBarView {
    pub theme: Theme,
}
impl StatusBarView {
    pub fn themed(theme: &Theme) -> Self {
        Self {
            theme: theme.clone(),
        }
    }
    pub fn build(self, left: View, right: View) -> View {
        let spacer = Container::empty().style(|s| s.grow = 1.0).build();
        row([left, spacer, right])
            .background(self.theme.color(ColorRole::SurfaceRaised))
            .style(|s| {
                s.padding = Edges::all(8.0);
                s.gap = Size::new(8.0, 0.0);
            })
            .build()
    }
}
