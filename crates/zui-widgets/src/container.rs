//! Layout builders own descriptions only; the runtime owns children and geometry.
use alloc::vec::Vec;
use zui_api::{
    layout::{LayoutKind, LayoutStyle, Track},
    view::{Key, View, ViewSpec},
    widget::{Invalidation, PaintContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_types::{Color, Point, Rect, Size};

/// A box around zero or one child. Row/column/grid/stack builders use the same surface
/// behavior, with different layout kinds. Backgrounds cover the border box.
pub struct Container {
    style: LayoutStyle,
    children: Vec<View>,
    key: Option<Key>,
    clip: bool,
    kind: LayoutKind,
    background: Option<Color>,
    border: Option<Color>,
    radius: u16,
}
impl Container {
    pub fn new(child: View) -> Self {
        Self::layout(LayoutKind::Column, [child])
    }
    pub fn empty() -> Self {
        Self::layout(LayoutKind::Column, [])
    }
    fn layout(kind: LayoutKind, children: impl IntoIterator<Item = View>) -> Self {
        Self {
            style: LayoutStyle {
                kind,
                ..Default::default()
            },
            children: children.into_iter().collect(),
            key: None,
            clip: false,
            kind,
            background: None,
            border: None,
            radius: 0,
        }
    }
    /// Edit size, padding, alignment, tracks and gaps without erasing defaults.
    /// The constructor owns `kind`; changing it here has no effect.
    pub fn style(mut self, configure: impl FnOnce(&mut LayoutStyle)) -> Self {
        configure(&mut self.style);
        self.style.kind = self.kind;
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    /// Rounded surface only; use padding to keep children inside the border.
    pub fn radius(mut self, radius: u16) -> Self {
        self.radius = radius;
        self
    }
    pub fn border(mut self, color: Color) -> Self {
        self.border = Some(color);
        self
    }
    pub fn keyed(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn clipped(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }
    pub fn build(self) -> View {
        let mut view = View::new(SurfaceView(self.background, self.border, self.radius))
            .styled(self.style)
            .clipped(self.clip)
            .with_children(self.children);
        view.key = self.key;
        view
    }
}
impl From<Container> for View {
    fn from(container: Container) -> Self {
        container.build()
    }
}

pub fn row(children: impl IntoIterator<Item = View>) -> Container {
    Container::layout(LayoutKind::Row, children)
}
pub fn column(children: impl IntoIterator<Item = View>) -> Container {
    Container::layout(LayoutKind::Column, children)
}
/// Back-to-front child order. Uses the runtime's ordinary paint/hit traversal.
pub fn stack(children: impl IntoIterator<Item = View>) -> Container {
    Container::layout(LayoutKind::Stack, children)
}
pub fn grid(
    columns: impl IntoIterator<Item = Track>,
    children: impl IntoIterator<Item = View>,
) -> Container {
    Container::layout(LayoutKind::Grid, children)
        .style(|style| style.columns = columns.into_iter().collect())
}

struct Surface(Option<Color>, Option<Color>, u16);
impl Widget for Surface {
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        if let Some(color) = self.0 {
            painter.fill(
                Path::rounded_rectangle(
                    Rect::new(0.0, 0.0, context.size.width, context.size.height),
                    f64::from(self.2),
                )
                .expect("valid surface bounds"),
                color,
            );
        }
        if let Some(color) = self.1
            && context.size.width > 1.0
            && context.size.height > 1.0
        {
            painter.stroke(
                Path::rounded_rectangle(
                    Rect::new(
                        0.5,
                        0.5,
                        context.size.width - 1.0,
                        context.size.height - 1.0,
                    ),
                    f64::from(self.2),
                )
                .expect("valid surface border"),
                color,
                1.0,
            );
        }
    }
    // Layout-only surfaces do not swallow input in empty areas. Descendants
    // still participate in the runtime's ordinary reverse-order hit traversal.
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
}
struct SurfaceView(Option<Color>, Option<Color>, u16);
impl ViewSpec for SurfaceView {
    type Widget = Surface;
    fn build(&self) -> Surface {
        Surface(self.0, self.1, self.2)
    }
    fn update(&self, widget: &mut Surface) -> Invalidation {
        if (widget.0, widget.1, widget.2) == (self.0, self.1, self.2) {
            Invalidation::None
        } else {
            *widget = Surface(self.0, self.1, self.2);
            Invalidation::Paint
        }
    }
}
