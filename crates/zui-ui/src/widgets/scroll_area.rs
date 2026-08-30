use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{RenderBuildContext, Widget, WidgetId},
};
use zui_core::{Dip, Point, Rect, Size};
use zui_platform::InputEvent;
use zui_render::{ClipShape, RenderNode, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
    Both,
}

/// A clipped viewport that translates one retained child in response to wheel input.
pub struct ScrollArea {
    id: WidgetId,
    child: Box<dyn Widget>,
    axis: ScrollAxis,
    offset: Point,
    content_size: Size,
    width: Option<Dip>,
    height: Option<Dip>,
    bounds: Rect,
    dragging_vertical_thumb: bool,
}
impl ScrollArea {
    pub fn vertical(child: impl Widget + 'static) -> Self {
        Self::new(child, ScrollAxis::Vertical)
    }
    pub fn horizontal(child: impl Widget + 'static) -> Self {
        Self::new(child, ScrollAxis::Horizontal)
    }
    pub fn both(child: impl Widget + 'static) -> Self {
        Self::new(child, ScrollAxis::Both)
    }
    fn new(child: impl Widget + 'static, axis: ScrollAxis) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            axis,
            offset: Point::default(),
            content_size: Size::ZERO,
            width: None,
            height: None,
            bounds: Rect::default(),
            dragging_vertical_thumb: false,
        }
    }
    pub fn width(mut self, value: f32) -> Self {
        self.width = Some(Dip(value.max(0.0)));
        self
    }
    pub fn height(mut self, value: f32) -> Self {
        self.height = Some(Dip(value.max(0.0)));
        self
    }
    pub fn offset(&self) -> Point {
        self.offset
    }
    fn clamp_offset(&mut self) {
        self.offset.x = Dip(self.offset.x.0.clamp(
            0.0,
            (self.content_size.width.0 - self.bounds.size.width.0).max(0.0),
        ));
        self.offset.y = Dip(self.offset.y.0.clamp(
            0.0,
            (self.content_size.height.0 - self.bounds.size.height.0).max(0.0),
        ));
    }
    fn place_child(&mut self) {
        self.clamp_offset();
        self.child.arrange(Rect {
            origin: Point {
                x: Dip(self.bounds.origin.x.0 - self.offset.x.0),
                y: Dip(self.bounds.origin.y.0 - self.offset.y.0),
            },
            size: self.content_size,
        });
    }
    fn vertical_thumb(&self) -> Option<Rect> {
        if !matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both)
            || self.content_size.height.0 <= self.bounds.size.height.0
        {
            return None;
        }
        let track = self.bounds.size.height.0;
        let thumb = (track * track / self.content_size.height.0).clamp(20.0, track);
        let max_offset = self.content_size.height.0 - track;
        let travel = track - thumb;
        Some(Rect {
            origin: Point {
                x: Dip(self.bounds.origin.x.0 + self.bounds.size.width.0 - 8.0),
                y: Dip(self.bounds.origin.y.0 + travel * self.offset.y.0 / max_offset),
            },
            size: Size {
                width: Dip(6.0),
                height: Dip(thumb),
            },
        })
    }
    fn scroll_from_thumb_y(&mut self, y: f32) {
        if let Some(thumb) = self.vertical_thumb() {
            let track = self.bounds.size.height.0;
            let travel = (track - thumb.size.height.0).max(1.0);
            let position =
                (y - self.bounds.origin.y.0 - thumb.size.height.0 / 2.0).clamp(0.0, travel);
            self.offset.y = Dip(position / travel * (self.content_size.height.0 - track));
            self.place_child();
        }
    }
}
impl Widget for ScrollArea {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        // The viewport belongs to the parent layout; only the scroll axis is
        // unbounded for the child. Never derive a vertical viewport width from
        // a long line of text, or the viewport and its scrollbar drift apart.
        let viewport = Size {
            width: self
                .width
                .unwrap_or(if matches!(self.axis, ScrollAxis::Vertical) {
                    constraints.max.width
                } else {
                    self.content_size.width
                }),
            height: self
                .height
                .unwrap_or(if matches!(self.axis, ScrollAxis::Horizontal) {
                    constraints.max.height
                } else {
                    self.content_size.height
                }),
        };
        let max = Size {
            width: if matches!(self.axis, ScrollAxis::Horizontal | ScrollAxis::Both) {
                Dip(1_000_000.0)
            } else {
                constraints.max.width
            },
            height: if matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both) {
                Dip(1_000_000.0)
            } else {
                constraints.max.height
            },
        };
        let max = Size {
            width: if matches!(self.axis, ScrollAxis::Vertical) {
                viewport.width
            } else {
                max.width
            },
            height: if matches!(self.axis, ScrollAxis::Horizontal) {
                viewport.height
            } else {
                max.height
            },
        };
        self.content_size = self.child.measure(Constraints::loose(max));
        let size = constraints.constrain(Size {
            width: self
                .width
                .unwrap_or(if matches!(self.axis, ScrollAxis::Vertical) {
                    constraints.max.width
                } else {
                    self.content_size.width
                }),
            height: self
                .height
                .unwrap_or(if matches!(self.axis, ScrollAxis::Horizontal) {
                    constraints.max.height
                } else {
                    self.content_size.height
                }),
        });
        self.bounds.size = size;
        size
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.place_child();
    }
    fn next_redraw(&self) -> Option<std::time::Instant> {
        self.child.next_redraw()
    }
    fn event(&mut self, event: &UiEvent, ctx: &mut EventContext) -> EventResult {
        if let Some(point) = event.position {
            if let InputEvent::MouseInput {
                button: zui_platform::MouseButton::Left,
                state: zui_platform::KeyState::Pressed,
            } = event.input
            {
                if self
                    .vertical_thumb()
                    .is_some_and(|thumb| thumb.contains(point))
                {
                    self.dragging_vertical_thumb = true;
                    self.scroll_from_thumb_y(point.y.0);
                    self.invalidate(ctx, self.bounds);
                    return EventResult::RequestRedraw;
                }
            }
            if matches!(
                event.input,
                InputEvent::MouseInput {
                    state: zui_platform::KeyState::Released,
                    ..
                }
            ) {
                self.dragging_vertical_thumb = false;
            }
            if self.dragging_vertical_thumb && matches!(event.input, InputEvent::CursorMoved { .. })
            {
                self.scroll_from_thumb_y(point.y.0);
                self.invalidate(ctx, self.bounds);
                return EventResult::RequestRedraw;
            }
        }
        if let InputEvent::MouseWheel { delta_x, delta_y } = event.input {
            let before = self.offset;
            if matches!(self.axis, ScrollAxis::Horizontal | ScrollAxis::Both) {
                self.offset.x = Dip(self.offset.x.0 - delta_x.0);
            }
            if matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both) {
                self.offset.y = Dip(self.offset.y.0 - delta_y.0);
            }
            self.place_child();
            if self.offset != before {
                self.invalidate(ctx, self.bounds);
                return EventResult::RequestRedraw;
            }
        }
        self.child.event(event, ctx)
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.child.set_theme(theme);
    }
    fn build_render_node(&self, theme: &Theme) -> RenderNode {
        let mut node = RenderNode::for_widget(self.bounds);
        node.set_id(self.id.0);
        node.set_clip(Some(ClipShape::Rect(Rect {
            origin: Point::default(),
            size: self.bounds.size,
        })));
        node.add_child(self.child.build_render_node(theme));
        if let Some(thumb) = self.vertical_thumb() {
            node.commands_mut()
                .push(zui_render::PaintCommand::RoundedRect {
                    rect: Rect {
                        origin: Point {
                            x: Dip(thumb.origin.x.0 - self.bounds.origin.x.0),
                            y: Dip(thumb.origin.y.0 - self.bounds.origin.y.0),
                        },
                        size: thumb.size,
                    },
                    radius: Dip(3.0),
                    color: zui_core::Color {
                        r: 0.75,
                        g: 0.78,
                        b: 0.84,
                        a: 0.75,
                    },
                });
        }
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
        node.set_clip(Some(ClipShape::Rect(Rect {
            origin: Point::default(),
            size: self.bounds.size,
        })));
        // Child bounds include the current scroll offset. Even when the
        // child content itself is clean, its retained transform changed, so
        // it must not reuse the previous subtree unchanged.
        let mut child_context = context
            .child(
                0,
                Transform::translate(self.bounds.origin.x, self.bounds.origin.y),
            )
            .force_rebuild_subtree();
        node.add_child(self.child.build_render_node_incremental(&mut child_context));
        if let Some(thumb) = self.vertical_thumb() {
            node.commands_mut()
                .push(zui_render::PaintCommand::RoundedRect {
                    rect: Rect {
                        origin: Point {
                            x: Dip(thumb.origin.x.0 - self.bounds.origin.x.0),
                            y: Dip(thumb.origin.y.0 - self.bounds.origin.y.0),
                        },
                        size: thumb.size,
                    },
                    radius: Dip(3.0),
                    color: zui_core::Color {
                        r: 0.75,
                        g: 0.78,
                        b: 0.84,
                        a: 0.75,
                    },
                });
        }
        node
    }
}
