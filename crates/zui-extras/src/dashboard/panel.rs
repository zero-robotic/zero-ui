use alloc::vec::Vec;
use zui_api::{
    layout::{Edges, LayoutError, LayoutKind, LayoutStyle},
    view::{Key, View, ViewSpec},
    widget::{Invalidation, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, NodeId, Point, Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelStyle {
    pub background: Color,
    pub border: Color,
    pub accent: Color,
    pub corner: f64,
    pub border_width: f64,
    pub accent_width: f64,
    pub accent_length: f64,
}
impl PanelStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            background: theme.color(ColorRole::Surface),
            border: theme.color(ColorRole::Border),
            accent: theme.color(ColorRole::Accent),
            corner: 12.0,
            border_width: 1.0,
            accent_width: 2.0,
            accent_length: 36.0,
        }
    }
    fn valid(self) -> bool {
        [
            self.corner,
            self.border_width,
            self.accent_width,
            self.accent_length,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0 && *v <= f32::MAX as f64)
    }
    fn validate(self, id: NodeId) -> Result<(), LayoutError> {
        if self.valid() {
            Ok(())
        } else {
            Err(LayoutError::InvalidStyle(id))
        }
    }
}
/// Chamfered decoration behind ordinary Column children. Descendant clipping is
/// rectangular, not a polygon mask; padding keeps content clear of the corners.
pub struct DashboardPanelView {
    pub children: Vec<View>,
    pub style: PanelStyle,
    pub layout: LayoutStyle,
    pub key: Option<Key>,
    pub clip: bool,
}
impl DashboardPanelView {
    pub fn themed(theme: &Theme, children: impl IntoIterator<Item = View>) -> Self {
        Self {
            children: children.into_iter().collect(),
            style: PanelStyle::from_theme(theme),
            layout: LayoutStyle {
                padding: Edges::all(20.0),
                gap: Size::new(0.0, 12.0),
                ..Default::default()
            },
            key: None,
            clip: true,
        }
    }
    pub fn build(self) -> View {
        let mut layout = self.layout;
        layout.kind = LayoutKind::Column;
        let mut view = View::new(PanelSurface(self.style))
            .styled(layout)
            .clipped(self.clip)
            .with_children(self.children);
        view.key = self.key;
        view
    }
}
#[derive(Clone, Copy)]
struct PanelSurface(PanelStyle);
impl ViewSpec for PanelSurface {
    type Widget = Self;
    fn build(&self) -> Self {
        *self
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        if self.0 == widget.0 {
            return Invalidation::None;
        }
        let invalid = !self.0.valid() || !widget.0.valid();
        *widget = *self;
        if invalid {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl Widget for PanelSurface {
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.0.validate(context.id)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let s = self.0;
        let size = context.size;
        if !s.valid() || size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let border = s.border_width.min(size.width).min(size.height);
        let accent = s.accent_width.min(size.width).min(size.height);
        let inset = border.max(if s.accent_length > 0.0 { accent } else { 0.0 }) / 2.0;
        let left = inset;
        let top = inset;
        let right = size.width - inset;
        let bottom = size.height - inset;
        let corner = s.corner.min((right - left) / 2.0).min((bottom - top) / 2.0);
        let mut path = Path::new();
        for (index, point) in [
            Point::new(left + corner, top),
            Point::new(right - corner, top),
            Point::new(right, top + corner),
            Point::new(right, bottom - corner),
            Point::new(right - corner, bottom),
            Point::new(left + corner, bottom),
            Point::new(left, bottom - corner),
            Point::new(left, top + corner),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        path.close();
        painter.with_clip(
            Path::rectangle(Rect::new(0.0, 0.0, size.width, size.height)),
            |painter| {
                painter.fill(path.clone(), s.background);
                if border > 0.0 {
                    painter.stroke(path, s.border, border);
                }
                let length = s.accent_length.min((right - left - corner * 2.0).max(0.0));
                if accent > 0.0 && length > 0.0 {
                    let mut lines = Path::new();
                    lines.move_to(Point::new(left + corner, top));
                    lines.line_to(Point::new(left + corner + length, top));
                    lines.move_to(Point::new(right - corner - length, bottom));
                    lines.line_to(Point::new(right - corner, bottom));
                    painter.stroke(lines, s.accent, accent);
                }
            },
        );
    }
}
