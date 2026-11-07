//! Layout builders own descriptions only; the runtime owns children and geometry.
use alloc::vec::Vec;
use zui_api::{
    layout::{LayoutKind, LayoutStyle, Track},
    view::{Key, View, ViewSpec},
    widget::{Invalidation, PaintContext, Widget},
};
use zui_paint::Painter;
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
    pub fn keyed(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn clipped(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }
    pub fn build(self) -> View {
        let mut view = View::new(SurfaceView(self.background))
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

struct Surface(Option<Color>);
impl Widget for Surface {
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        if let Some(color) = self.0 {
            painter.fill_rect(
                Rect::new(0.0, 0.0, context.size.width, context.size.height),
                color,
            );
        }
    }
    // Layout-only surfaces do not swallow input in empty areas. Descendants
    // still participate in the runtime's ordinary reverse-order hit traversal.
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
}
struct SurfaceView(Option<Color>);
impl ViewSpec for SurfaceView {
    type Widget = Surface;
    fn build(&self) -> Surface {
        Surface(self.0)
    }
    fn update(&self, widget: &mut Surface) -> Invalidation {
        if widget.0 == self.0 {
            Invalidation::None
        } else {
            widget.0 = self.0;
            Invalidation::Paint
        }
    }
}
