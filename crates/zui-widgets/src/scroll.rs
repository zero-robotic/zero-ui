//! Scroll behavior owns offsets; layout supplies extents, core presents children.
mod virtual_list;
use crate::axis::Axis;
pub use crate::axis::ScrollAxes;
use crate::scrollbar as bar;
pub use crate::scrollbar::ScrollbarStyle;
use alloc::vec;
pub use virtual_list::{VirtualList, VirtualListNavigate, VirtualListView};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Align, Dimension, Justify, LayoutError, LayoutKind, LayoutStyle, Overflow, Track},
    view::{Key as ViewKey, View, ViewSpec},
    widget::{ChildViewport, Invalidation, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_types::{Color, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollChanged {
    pub offset: Point,
    pub max_offset: Point,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollRequest {
    offset: Point,
    revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Properties {
    axes: ScrollAxes,
    line_step: f64,
    background: Option<Color>,
    focus_color: Color,
    disabled: bool,
    focusable: bool,
    action: Option<ActionId>,
    request: Option<ScrollRequest>,
    scrollbar: Option<ScrollbarStyle>,
}
impl Default for Properties {
    fn default() -> Self {
        Self {
            axes: ScrollAxes::Vertical,
            line_step: 40.0,
            background: None,
            focus_color: Color::rgb(41, 210, 230),
            disabled: false,
            focusable: true,
            action: None,
            request: None,
            scrollbar: Some(ScrollbarStyle::default()),
        }
    }
}
/// A single content child. Scroll axes use intrinsic dimensions; other axes fit
/// the viewport. The builder owns structural layout fields, not application sizing.
pub struct ScrollView {
    content: View,
    style: LayoutStyle,
    key: Option<ViewKey>,
    properties: Properties,
}
impl ScrollView {
    pub fn vertical(content: View) -> Self {
        Self::new(content, ScrollAxes::Vertical)
    }
    pub fn horizontal(content: View) -> Self {
        Self::new(content, ScrollAxes::Horizontal)
    }
    pub fn both(content: View) -> Self {
        Self::new(content, ScrollAxes::Both)
    }
    fn new(content: View, axes: ScrollAxes) -> Self {
        Self {
            content,
            style: LayoutStyle::default(),
            key: None,
            properties: Properties {
                axes,
                ..Default::default()
            },
        }
    }
    pub fn style(mut self, configure: impl FnOnce(&mut LayoutStyle)) -> Self {
        configure(&mut self.style);
        self
    }
    pub fn keyed(mut self, key: impl Into<ViewKey>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.properties.background = Some(color);
        self
    }
    pub fn focus_color(mut self, color: Color) -> Self {
        self.properties.focus_color = color;
        self
    }
    /// Overlay bars do not change content size; None retains wheel and keyboard scrolling.
    pub fn scrollbar(mut self, style: Option<ScrollbarStyle>) -> Self {
        self.properties.scrollbar = style;
        self
    }
    /// Allow descendants to receive focus without making the scroll surface a Tab stop.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.properties.focusable = focusable;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.properties.disabled = disabled;
        self
    }
    pub fn line_step(mut self, step: f64) -> Self {
        self.properties.line_step = step;
        self
    }
    pub fn on_scroll(mut self, action: ActionId) -> Self {
        self.properties.action = Some(action);
        self
    }
    /// A changed offset or revision requests a jump. Unused axes clamp to zero.
    pub fn scroll_to(mut self, offset: Point, revision: u64) -> Self {
        self.properties.request = Some(ScrollRequest { offset, revision });
        self
    }
    pub fn build(mut self) -> View {
        let horizontal = self.properties.axes.contains(Axis::Horizontal);
        let vertical = self.properties.axes.contains(Axis::Vertical);
        self.style.kind = LayoutKind::Column;
        self.style.align = Align::Start;
        self.style.justify = Justify::Start;
        self.style.wrap = false;
        self.style.gap = Size::ZERO;
        self.style.rows.clear();
        self.style.columns.clear();
        self.style.overflow_x = if horizontal {
            Overflow::Scroll
        } else {
            Overflow::Clip
        };
        self.style.overflow_y = if vertical {
            Overflow::Scroll
        } else {
            Overflow::Clip
        };
        // Never replace the application's own child style. The wrapper controls
        // content sizing: intrinsic width on x, unconstrained auto height on y.
        let content = crate::grid([Track::Fraction(1.0)], [self.content])
            .style(|s| {
                s.width = if horizontal {
                    Dimension::MaxContent
                } else {
                    Dimension::Percent(1.0)
                };
                s.height = if vertical {
                    Dimension::Auto
                } else {
                    Dimension::Percent(1.0)
                };
                s.shrink = 0.0;
            })
            .build();
        let mut view = View::new(self.properties)
            .styled(self.style)
            .with_children([content]);
        view.key = self.key;
        view
    }
}
impl From<ScrollView> for View {
    fn from(view: ScrollView) -> Self {
        view.build()
    }
}
#[derive(Clone, Copy)]
struct Grab {
    axis: Axis,
    fraction: f64,
}
/// Persistent state shared by all axis configurations.
pub struct Scroll {
    properties: Properties,
    offset: Point,
    max_offset: Point,
    page: Size,
    pending: bool,
    focused: bool,
    focus_visible: bool,
    grab: Option<Grab>,
}
impl Scroll {
    pub fn offset(&self) -> Point {
        self.offset
    }
    pub fn max_offset(&self) -> Point {
        self.max_offset
    }
    pub fn axes(&self) -> ScrollAxes {
        self.properties.axes
    }
    // Shared interaction state accepts logical extents without sending large
    // coordinates through the layout engine. Concrete containers own the source.
    fn prepare_range(
        &mut self,
        context: PrepareContext,
        max_offset: Point,
    ) -> Result<(), LayoutError> {
        if !self.properties.line_step.is_finite()
            || self.properties.line_step <= 0.0
            || self
                .properties
                .request
                .is_some_and(|r| !r.offset.is_finite() || r.offset.x < 0.0 || r.offset.y < 0.0)
            || self.properties.scrollbar.is_some_and(|s| !s.valid())
        {
            return Err(LayoutError::InvalidStyle(context.id));
        }
        for axis in Axis::ALL {
            axis.set(
                &mut self.max_offset,
                if self.properties.axes.contains(axis) {
                    axis.value(max_offset)
                } else {
                    0.0
                },
            );
        }
        self.page = context.content_size;
        if self.pending {
            if let Some(request) = self.properties.request {
                self.offset = request.offset;
            }
            self.pending = false;
        }
        self.offset = self.clamp(self.offset);
        Ok(())
    }
    fn clamp(&self, point: Point) -> Point {
        Point::new(
            point.x.clamp(0.0, self.max_offset.x),
            point.y.clamp(0.0, self.max_offset.y),
        )
    }
    fn bars(&self, content: Rect) -> [Option<bar::BarGeometry>; 2] {
        let Some(style) = self.properties.scrollbar else {
            return [None, None];
        };
        Axis::ALL.map(|axis| {
            let other = axis.other();
            let shorten = if other.value(self.max_offset) > 0.0 {
                style.width.min(axis.length(content.size))
            } else {
                0.0
            };
            style.geometry(
                axis,
                content,
                axis.value(self.offset),
                axis.value(self.max_offset),
                shorten,
            )
        })
    }
    fn move_to(&mut self, offset: Point) -> EventResponse {
        let offset = self.clamp(offset);
        if offset == self.offset {
            return EventResponse::default();
        }
        self.offset = offset;
        EventResponse {
            handled: true,
            stop_propagation: true,
            invalidation: Invalidation::Paint,
            actions: self
                .properties
                .action
                .map(|id| {
                    vec![WidgetAction::new(
                        id,
                        ScrollChanged {
                            offset,
                            max_offset: self.max_offset,
                        },
                    )]
                })
                .unwrap_or_default(),
            ..Default::default()
        }
    }
    fn move_axis(&mut self, axis: Axis, value: f64) -> EventResponse {
        let mut next = self.offset;
        axis.set(&mut next, value);
        self.move_to(next)
    }
    fn bar_event(&mut self, context: EventContext, event: &WidgetEvent) -> Option<EventResponse> {
        if context.phase != EventPhase::Target {
            return None;
        }
        let bars = context
            .geometry
            .map(|g| self.bars(g.content_bounds))
            .unwrap_or([None, None]);
        if let Some(grab) = self.grab {
            match event {
                WidgetEvent::Input(InputEvent::PointerMoved(_))
                | WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }) => {
                    let bar = bars[grab.axis.index()];
                    let release = bar.is_none()
                        || matches!(
                            event,
                            WidgetEvent::Input(InputEvent::PointerButton { pressed: false, .. })
                        );
                    let next = bar.zip(context.local_position).and_then(|(bar, point)| {
                        bar.drag_offset(point, grab.fraction, grab.axis.value(self.max_offset))
                    });
                    let mut response = next
                        .map(|next| self.move_axis(grab.axis, next))
                        .unwrap_or_default();
                    response.handled = true;
                    response.stop_propagation = true;
                    response.invalidation = Invalidation::Paint;
                    if release {
                        self.grab = None;
                        response.capture = CaptureRequest::Release;
                    }
                    return Some(response);
                }
                _ => {}
            }
        }
        let point = context.local_position?;
        if !matches!(
            event,
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            })
        ) {
            return None;
        }
        let bar = bars
            .into_iter()
            .flatten()
            .find(|bar| bar.track.contains(point))?;
        let mut response = if bar.thumb.contains(point) {
            if bar.travel > 0.0 {
                self.grab = Some(Grab {
                    axis: bar.axis,
                    fraction: bar.grab_fraction(point),
                });
                EventResponse {
                    capture: CaptureRequest::Capture,
                    ..Default::default()
                }
            } else {
                EventResponse::default()
            }
        } else {
            let direction = if bar.axis.value(point) < bar.axis.value(bar.thumb.origin) {
                -1.0
            } else {
                1.0
            };
            self.move_axis(
                bar.axis,
                bar.axis.value(self.offset) + direction * bar.axis.length(self.page),
            )
        };
        response.handled = true;
        response.stop_propagation = true;
        response.focus = if self.properties.focusable {
            FocusRequest::Focus
        } else {
            FocusRequest::None
        };
        response.invalidation = Invalidation::Paint;
        Some(response)
    }
}
impl ViewSpec for Properties {
    type Widget = Scroll;
    fn build(&self) -> Scroll {
        Scroll {
            properties: *self,
            offset: Point::ZERO,
            max_offset: Point::ZERO,
            page: Size::ZERO,
            pending: true,
            focused: false,
            focus_visible: false,
            grab: None,
        }
    }
    fn update(&self, widget: &mut Scroll) -> Invalidation {
        if *self == widget.properties {
            return Invalidation::None;
        }
        let reprepare = self.axes != widget.properties.axes
            || self.request != widget.properties.request
            || self.line_step != widget.properties.line_step
            || self.scrollbar.map(|s| (s.width, s.min_thumb))
                != widget.properties.scrollbar.map(|s| (s.width, s.min_thumb));
        widget.pending |= self.request != widget.properties.request;
        widget.properties = *self;
        if reprepare {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl Widget for Scroll {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        !self.properties.disabled && self.properties.focusable
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        let mut max_offset = Point::ZERO;
        for axis in Axis::ALL {
            let max = (axis.length(context.layout.scroll_extent)
                - axis.length(context.layout.bounds.size))
            .max(0.0);
            axis.set(&mut max_offset, max);
        }
        self.prepare_range(context, max_offset)
    }
    fn child_viewport(&self, context: PaintContext) -> ChildViewport {
        ChildViewport {
            translation: Point::new(-self.offset.x, -self.offset.y),
            clip: Some(context.content_bounds),
        }
    }
    fn reveal_child(&mut self, context: PaintContext, area: Rect) -> bool {
        if !self.enabled()
            || context.content_bounds.size.width <= 0.0
            || context.content_bounds.size.height <= 0.0
        {
            return false;
        }
        let mut next = self.offset;
        for axis in Axis::ALL {
            if self.properties.axes.contains(axis) {
                axis.set(
                    &mut next,
                    axis.value(self.offset) + axis.reveal_delta(context.content_bounds, area),
                );
            }
        }
        let next = self.clamp(next);
        let changed = next != self.offset;
        self.offset = next;
        changed
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        if let Some(color) = self.properties.background {
            painter.fill_rect(
                Rect::new(0.0, 0.0, context.size.width, context.size.height),
                color,
            );
        }
    }
    fn paint_foreground(&self, context: PaintContext, painter: &mut Painter<'_>) {
        for bar in self.bars(context.content_bounds).into_iter().flatten() {
            let style = self.properties.scrollbar.expect("bar has style");
            painter.fill_rect(bar.track, style.track);
            let color = if !self.enabled() {
                style.disabled
            } else if self.grab.is_some_and(|g| g.axis == bar.axis) {
                style.dragging
            } else {
                style.thumb
            };
            painter.fill_rect(bar.thumb, color);
        }
        if self.focused
            && self.focus_visible
            && self.enabled()
            && context.size.width >= 2.0
            && context.size.height >= 2.0
        {
            painter.stroke(
                Path::rectangle(Rect::new(
                    0.5,
                    0.5,
                    context.size.width - 1.0,
                    context.size.height - 1.0,
                )),
                self.properties.focus_color,
                1.0,
            );
        }
    }
    fn hit_test_foreground(&self, point: Point, context: PaintContext) -> bool {
        self.bars(context.content_bounds)
            .into_iter()
            .flatten()
            .any(|bar| bar.track.contains(point))
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        if matches!(event, WidgetEvent::PointerCaptureLost) {
            let active = self.grab.take().is_some();
            return EventResponse {
                invalidation: if active {
                    Invalidation::Paint
                } else {
                    Invalidation::None
                },
                ..Default::default()
            };
        }
        if let WidgetEvent::FocusChanged(focused) = event {
            self.focused = *focused;
            self.focus_visible = *focused;
            return EventResponse {
                invalidation: Invalidation::Paint,
                ..Default::default()
            };
        }
        if let WidgetEvent::FocusVisibleChanged(visible) = event {
            self.focus_visible = *visible;
            return EventResponse {
                invalidation: Invalidation::Paint,
                ..Default::default()
            };
        }
        if !self.enabled() || !matches!(context.phase, EventPhase::Target | EventPhase::Bubble) {
            return EventResponse::default();
        }
        if let Some(response) = self.bar_event(context, event) {
            return response;
        }
        if self.page.width <= 0.0 || self.page.height <= 0.0 {
            return EventResponse::default();
        }
        match event {
            WidgetEvent::Input(InputEvent::Scroll(delta)) => {
                let (x, y, factor) = match delta {
                    ScrollDelta::Lines { x, y } => (*x, *y, self.properties.line_step),
                    ScrollDelta::Pixels { x, y } => (*x, *y, 1.0),
                };
                if !x.is_finite() || !y.is_finite() {
                    return EventResponse::default();
                }
                self.move_to(Point::new(
                    self.offset.x - x * factor,
                    self.offset.y - y * factor,
                ))
            }
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            }) if context.phase == EventPhase::Target => EventResponse {
                handled: true,
                focus: if self.properties.focusable {
                    FocusRequest::Focus
                } else {
                    FocusRequest::None
                },
                ..Default::default()
            },
            WidgetEvent::Input(InputEvent::Key(key))
                if context.phase == EventPhase::Target
                    && key.pressed
                    && !key.modifiers.control
                    && !key.modifiers.alt
                    && !key.modifiers.super_key
                    && !key.modifiers.shift =>
            {
                let primary = if self.properties.axes == ScrollAxes::Horizontal {
                    Axis::Horizontal
                } else {
                    Axis::Vertical
                };
                let (axis, next) = match key.key {
                    Key::ArrowLeft => (Axis::Horizontal, self.offset.x - self.properties.line_step),
                    Key::ArrowRight => {
                        (Axis::Horizontal, self.offset.x + self.properties.line_step)
                    }
                    Key::ArrowUp => (Axis::Vertical, self.offset.y - self.properties.line_step),
                    Key::ArrowDown => (Axis::Vertical, self.offset.y + self.properties.line_step),
                    Key::PageUp => (
                        primary,
                        primary.value(self.offset) - primary.length(self.page),
                    ),
                    Key::PageDown => (
                        primary,
                        primary.value(self.offset) + primary.length(self.page),
                    ),
                    Key::Home => return self.move_to(Point::ZERO),
                    Key::End => return self.move_to(self.max_offset),
                    _ => return EventResponse::default(),
                };
                self.move_axis(axis, next)
            }
            _ => EventResponse::default(),
        }
    }
}
