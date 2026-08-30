use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{RenderBuildContext, Widget, WidgetId},
};
use zui_core::{Color, Dip, Point, Rect, Size};
use zui_render::{ClipShape, RenderNode, Transform};

/// Insets around a container's child or outside its painted surface.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeInsets {
    pub left: Dip,
    pub top: Dip,
    pub right: Dip,
    pub bottom: Dip,
}

impl EdgeInsets {
    pub const ZERO: Self = Self::all(Dip::ZERO);

    pub const fn all(value: Dip) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub const fn symmetric(horizontal: Dip, vertical: Dip) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }

    pub const fn new(left: Dip, top: Dip, right: Dip, bottom: Dip) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    fn horizontal(self) -> f32 {
        self.left.0.max(0.0) + self.right.0.max(0.0)
    }
    fn vertical(self) -> f32 {
        self.top.0.max(0.0) + self.bottom.0.max(0.0)
    }
}

/// Positions a child within the available content area.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ContainerAlignment {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl ContainerAlignment {
    fn factors(self) -> (f32, f32) {
        match self {
            Self::TopLeft => (0.0, 0.0),
            Self::Top => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::Left => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::Right => (1.0, 0.5),
            Self::BottomLeft => (0.0, 1.0),
            Self::Bottom => (0.5, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
    }
}

/// A styled, constrained, single-child widget.
///
/// `Container` owns decoration and box-model concerns: margin, padding,
/// minimum/maximum or fixed dimensions, child alignment, background, border,
/// corner radius, and optional child clipping. For arranging multiple children,
/// compose it with a layout widget such as [`crate::layouts::Layout`].
pub struct Container {
    id: WidgetId,
    child: Box<dyn Widget>,
    margin: EdgeInsets,
    padding: EdgeInsets,
    alignment: ContainerAlignment,
    width: Option<Dip>,
    height: Option<Dip>,
    min_width: Option<Dip>,
    min_height: Option<Dip>,
    max_width: Option<Dip>,
    max_height: Option<Dip>,
    background: Option<Color>,
    border: Option<(Dip, Color)>,
    radius: Dip,
    clip_child: bool,
    bounds: Rect,
}

impl Container {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            margin: EdgeInsets::ZERO,
            padding: EdgeInsets::ZERO,
            alignment: ContainerAlignment::TopLeft,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            background: None,
            border: None,
            radius: Dip::ZERO,
            clip_child: false,
            bounds: Rect::default(),
        }
    }

    pub fn child(&self) -> &dyn Widget {
        &*self.child
    }
    pub fn margin(mut self, value: impl Into<EdgeInsets>) -> Self {
        self.margin = value.into();
        self
    }
    pub fn padding(mut self, value: impl Into<EdgeInsets>) -> Self {
        self.padding = value.into();
        self
    }
    pub fn align(mut self, value: ContainerAlignment) -> Self {
        self.alignment = value;
        self
    }
    pub fn width(mut self, value: f32) -> Self {
        self.width = Some(Dip(value.max(0.0)));
        self
    }
    pub fn height(mut self, value: f32) -> Self {
        self.height = Some(Dip(value.max(0.0)));
        self
    }
    pub fn min_width(mut self, value: f32) -> Self {
        self.min_width = Some(Dip(value.max(0.0)));
        self
    }
    pub fn min_height(mut self, value: f32) -> Self {
        self.min_height = Some(Dip(value.max(0.0)));
        self
    }
    pub fn max_width(mut self, value: f32) -> Self {
        self.max_width = Some(Dip(value.max(0.0)));
        self
    }
    pub fn max_height(mut self, value: f32) -> Self {
        self.max_height = Some(Dip(value.max(0.0)));
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border = (width > 0.0).then_some((Dip(width), color));
        self
    }
    pub fn radius(mut self, value: f32) -> Self {
        self.radius = Dip(value.max(0.0));
        self
    }
    pub fn clip_child(mut self) -> Self {
        self.clip_child = true;
        self
    }

    fn border_width(&self) -> f32 {
        self.border.map_or(0.0, |(width, _)| width.0)
    }
    fn content_insets(&self) -> EdgeInsets {
        let border = self.border_width();
        EdgeInsets::new(
            Dip(self.margin.left.0.max(0.0) + self.padding.left.0.max(0.0) + border),
            Dip(self.margin.top.0.max(0.0) + self.padding.top.0.max(0.0) + border),
            Dip(self.margin.right.0.max(0.0) + self.padding.right.0.max(0.0) + border),
            Dip(self.margin.bottom.0.max(0.0) + self.padding.bottom.0.max(0.0) + border),
        )
    }
    fn painted_bounds(&self) -> Rect {
        inset(self.bounds, self.margin)
    }
    fn content_bounds(&self) -> Rect {
        inset(self.bounds, self.content_insets())
    }
    fn constrain_dimension(
        value: f32,
        min: Option<Dip>,
        max: Option<Dip>,
        fixed: Option<Dip>,
        parent_min: f32,
        parent_max: f32,
    ) -> Dip {
        let lower = min.map_or(parent_min, |v| parent_min.max(v.0));
        let upper = max.map_or(parent_max, |v| parent_max.min(v.0)).max(lower);
        Dip(fixed.map_or(value, |v| v.0).clamp(lower, upper))
    }
    fn decorate(&self, node: &mut RenderNode, theme: &Theme) {
        let paint = self.painted_bounds();
        let radius = Dip(self
            .radius
            .0
            .min(paint.size.width.0.min(paint.size.height.0) / 2.0));
        let mut ctx = crate::PaintContext::new_at(node.commands_mut(), theme, self.bounds.origin);
        if let Some((_, color)) = self.border {
            ctx.fill_rounded_rect(paint, radius, color);
        }
        if let Some(color) = self.background {
            let inner = inset(paint, EdgeInsets::all(Dip(self.border_width())));
            let inner_radius = Dip((radius.0 - self.border_width()).max(0.0));
            ctx.fill_rounded_rect(inner, inner_radius, color);
        }
        if self.clip_child {
            let local = Rect {
                origin: Point {
                    x: self.margin.left,
                    y: self.margin.top,
                },
                size: paint.size,
            };
            node.set_clip(Some(if radius.0 > 0.0 {
                ClipShape::RoundedRect {
                    rect: local,
                    radius,
                }
            } else {
                ClipShape::Rect(local)
            }));
        }
    }
}

impl From<Dip> for EdgeInsets {
    fn from(value: Dip) -> Self {
        Self::all(value)
    }
}
impl From<f32> for EdgeInsets {
    fn from(value: f32) -> Self {
        Self::all(Dip(value.max(0.0)))
    }
}

impl Widget for Container {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        let content = self.content_bounds();
        let child_size = self.child.bounds().size;
        let (x, y) = self.alignment.factors();
        self.child.arrange(Rect {
            origin: Point {
                x: Dip(
                    content.origin.x.0 + (content.size.width.0 - child_size.width.0).max(0.0) * x
                ),
                y: Dip(
                    content.origin.y.0 + (content.size.height.0 - child_size.height.0).max(0.0) * y
                ),
            },
            size: child_size,
        });
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        let insets = self.content_insets();
        let available = Size {
            width: Dip((constraints.max.width.0 - insets.horizontal()).max(0.0)),
            height: Dip((constraints.max.height.0 - insets.vertical()).max(0.0)),
        };
        let child = self.child.measure(Constraints::loose(available));
        let intrinsic = Size {
            width: Dip(child.width.0 + insets.horizontal()),
            height: Dip(child.height.0 + insets.vertical()),
        };
        let size = Size {
            width: Self::constrain_dimension(
                intrinsic.width.0,
                self.min_width,
                self.max_width,
                self.width,
                constraints.min.width.0,
                constraints.max.width.0,
            ),
            height: Self::constrain_dimension(
                intrinsic.height.0,
                self.min_height,
                self.max_height,
                self.height,
                constraints.min.height.0,
                constraints.max.height.0,
            ),
        };
        self.bounds.size = size;
        size
    }
    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.child.next_redraw()
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        self.child.event(event, ctx)
    }
    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_id(self.id.0);
        node.add_child(self.child.build_render_node(theme));
        self.decorate(&mut node, theme);
        node
    }
    fn build_render_node_incremental(&self, context: &mut RenderBuildContext<'_>) -> RenderNode {
        if !context.subtree_is_dirty(self.id) {
            if let Some(previous) = context.previous() {
                return previous.clone();
            }
        }
        let mut node = context.localize(RenderNode::for_widget(self.bounds));
        node.set_id(self.id.0);
        let mut child_context = context.child(
            0,
            Transform::translate(self.bounds.origin.x, self.bounds.origin.y),
        );
        node.add_child(self.child.build_render_node_incremental(&mut child_context));
        self.decorate(&mut node, context.theme());
        node
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.child.set_theme(theme);
    }
}

fn inset(rect: Rect, insets: EdgeInsets) -> Rect {
    let left = insets.left.0.max(0.0);
    let top = insets.top.0.max(0.0);
    let right = insets.right.0.max(0.0);
    let bottom = insets.bottom.0.max(0.0);
    Rect {
        origin: Point {
            x: Dip(rect.origin.x.0 + left),
            y: Dip(rect.origin.y.0 + top),
        },
        size: Size {
            width: Dip((rect.size.width.0 - left - right).max(0.0)),
            height: Dip((rect.size.height.0 - top - bottom).max(0.0)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn box_model_and_fixed_dimensions_are_applied() {
        let mut container = Container::new(Text::new("Hi"))
            .margin(2.0)
            .padding(EdgeInsets::symmetric(Dip(4.0), Dip(6.0)))
            .border(1.0, Color::BLACK)
            .width(100.0)
            .height(50.0);
        assert_eq!(
            container.measure(Constraints::loose(Size {
                width: Dip(200.0),
                height: Dip(200.0)
            })),
            Size {
                width: Dip(100.0),
                height: Dip(50.0)
            }
        );
        container.arrange(Rect {
            origin: Point::default(),
            size: Size {
                width: Dip(100.0),
                height: Dip(50.0),
            },
        });
        assert_eq!(
            container.child().bounds().origin,
            Point {
                x: Dip(7.0),
                y: Dip(9.0)
            }
        );
    }
}
