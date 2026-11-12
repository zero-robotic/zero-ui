use crate::NumericRange;
use crate::appearance::{Surface, VisualState};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{AvailableSpace, LayoutError},
    view::ViewSpec,
    widget::{Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, NodeId, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderChanged {
    pub value: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliderColors {
    pub track: Color,
    pub fill: Color,
    pub thumb: Color,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderStyle {
    pub normal: SliderColors,
    pub hovered: SliderColors,
    pub pressed: SliderColors,
    pub disabled: SliderColors,
    pub focus_ring: Color,
    pub thumb_size: f32,
    pub track_thickness: f32,
    pub natural_width: f32,
}
impl SliderStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let normal = SliderColors {
            track: theme.color(ColorRole::Border),
            fill: theme.color(ColorRole::Accent),
            thumb: theme.color(ColorRole::Text),
        };
        Self {
            normal,
            hovered: SliderColors {
                thumb: theme.color(ColorRole::Accent),
                ..normal
            },
            pressed: SliderColors {
                thumb: theme.color(ColorRole::OnAccent),
                ..normal
            },
            disabled: SliderColors {
                fill: theme.color(ColorRole::TextMuted),
                thumb: theme.color(ColorRole::TextMuted),
                ..normal
            },
            focus_ring: theme.color(ColorRole::Accent),
            thumb_size: if theme.controls().min_height > 0 {
                14.0
            } else {
                20.0
            },
            track_thickness: 4.0,
            natural_width: 200.0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderView {
    pub range: NumericRange,
    pub value: f64,
    pub action: ActionId,
    pub disabled: bool,
    pub style: SliderStyle,
}
impl SliderView {
    pub fn themed(theme: &Theme, range: NumericRange, value: f64, action: ActionId) -> Self {
        Self {
            range,
            value,
            action,
            disabled: false,
            style: SliderStyle::from_theme(theme),
        }
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        let s = self.style;
        if !self.range.contains(self.value)
            || !s.thumb_size.is_finite()
            || s.thumb_size < 4.0
            || !s.track_thickness.is_finite()
            || s.track_thickness <= 0.0
            || s.track_thickness > s.thumb_size
            || !s.natural_width.is_finite()
            || s.natural_width < s.thumb_size
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    offset: f64,
    last_request: Option<f64>,
    last_x: Option<f64>,
}
pub struct Slider {
    properties: SliderView,
    focused: bool,
    focus_visible: bool,
    hovered: bool,
    drag: Option<Drag>,
}
impl ViewSpec for SliderView {
    type Widget = Slider;
    fn build(&self) -> Slider {
        Slider {
            properties: *self,
            focused: false,
            focus_visible: false,
            hovered: false,
            drag: None,
        }
    }
    fn update(&self, widget: &mut Slider) -> Invalidation {
        let old = widget.properties;
        if *self == old {
            return Invalidation::None;
        }
        let geometry_changed = self.style.thumb_size != old.style.thumb_size
            || self.style.track_thickness != old.style.track_thickness
            || self.style.natural_width != old.style.natural_width;
        let acknowledged = widget
            .drag
            .is_some_and(|drag| drag.last_request == Some(self.value));
        if self.disabled
            || self.range != old.range
            || self.action != old.action
            || geometry_changed
            || (self.value != old.value && !acknowledged)
        {
            widget.drag = None;
        }
        widget.properties = *self;
        if geometry_changed || !self.range.contains(self.value) {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
/// Shared event/paint geometry; narrow content has a non-interactive zero-length track.
struct Track {
    start: f64,
    length: f64,
    y: f64,
    thumb: f64,
}
impl Track {
    fn new(bounds: Rect, style: SliderStyle) -> Self {
        let thumb = (style.thumb_size as f64)
            .min(bounds.size.width)
            .min(bounds.size.height);
        Self {
            start: bounds.origin.x + thumb / 2.0,
            length: (bounds.size.width - thumb).max(0.0),
            y: bounds.origin.y + bounds.size.height / 2.0,
            thumb,
        }
    }
    fn center(&self, properties: SliderView) -> f64 {
        self.start + properties.range.fraction(properties.value) * self.length
    }
}
impl Slider {
    fn request(&mut self, value: f64, response: &mut EventResponse) {
        let previous = self
            .drag
            .and_then(|drag| drag.last_request)
            .unwrap_or(self.properties.value);
        if previous != value {
            response.actions.push(WidgetAction::new(
                self.properties.action,
                SliderChanged { value },
            ));
            if let Some(drag) = &mut self.drag {
                drag.last_request = Some(value);
            }
        }
    }
    fn drag_to(&mut self, x: f64, track: &Track, response: &mut EventResponse) {
        if let Some(drag) = self.drag
            && track.length > 0.0
            && drag.last_x != Some(x)
        {
            if let Some(current) = &mut self.drag {
                current.last_x = Some(x);
            }
            self.request(
                self.properties
                    .range
                    .at((x - drag.offset - track.start) / track.length),
                response,
            );
        }
    }
}
impl Widget for Slider {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.validate(context.id)?;
        let preferred = match context.input.available_width {
            AvailableSpace::Definite(width) => self.properties.style.natural_width.min(width),
            AvailableSpace::MinContent => self.properties.style.thumb_size,
            AvailableSpace::MaxContent => self.properties.style.natural_width,
        };
        Ok(Size::new(
            context.input.known_width.unwrap_or(preferred) as f64,
            context
                .input
                .known_height
                .unwrap_or(self.properties.style.thumb_size) as f64,
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.properties.validate(context.id)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let p = self.properties;
        let state = VisualState {
            disabled: p.disabled,
            pressed: self.drag.is_some(),
            hovered: self.hovered,
            focus_visible: self.focused && self.focus_visible && !p.disabled,
        };
        let colors = state.resolve(
            p.style.normal,
            p.style.hovered,
            p.style.pressed,
            p.style.disabled,
        );
        let t = Track::new(context.content_bounds, p.style);
        let x = t.center(p);
        let thickness = (p.style.track_thickness as f64).min(t.thumb);
        painter.with_clip(Path::rectangle(context.content_bounds), |painter| {
            painter.fill(
                Path::rounded_rectangle(
                    Rect::new(t.start, t.y - thickness / 2.0, t.length, thickness),
                    thickness / 2.0,
                )
                .expect("valid slider track"),
                colors.track,
            );
            painter.fill(
                Path::rounded_rectangle(
                    Rect::new(t.start, t.y - thickness / 2.0, x - t.start, thickness),
                    thickness / 2.0,
                )
                .expect("valid slider fill"),
                colors.fill,
            );
            painter.fill(
                Path::rounded_rectangle(
                    Rect::new(x - t.thumb / 2.0, t.y - t.thumb / 2.0, t.thumb, t.thumb),
                    3.0,
                )
                .expect("valid slider thumb"),
                colors.thumb,
            );
        });
        Surface {
            bounds: Rect::new(0.0, 0.0, context.size.width, context.size.height),
            radius: 0.0,
        }
        .focus(painter, state, p.style.focus_ring);
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        let before = (self.focused, self.focus_visible, self.hovered, self.drag);
        let mut response = EventResponse::default();
        match event {
            WidgetEvent::FocusChanged(focused) => {
                self.focused = *focused;
                self.focus_visible = *focused;
                if !focused {
                    self.drag = None;
                }
            }
            WidgetEvent::PointerCaptureLost => self.drag = None,
            WidgetEvent::FocusVisibleChanged(visible) => self.focus_visible = *visible,
            WidgetEvent::Input(InputEvent::PointerEntered) => self.hovered = true,
            WidgetEvent::Input(InputEvent::PointerLeft) => self.hovered = false,
            _ => {}
        }
        if context.phase == EventPhase::Target {
            match event {
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed,
                    ..
                }) => {
                    response.handled = true;
                    if self.enabled()
                        && let Some(geometry) = context.geometry
                        && let Some(point) = context.local_position
                    {
                        let track = Track::new(geometry.content_bounds, self.properties.style);
                        if *pressed
                            && geometry.content_bounds.contains(point)
                            && track.length > 0.0
                            && track.thumb > 0.0
                        {
                            let center = track.center(self.properties);
                            let on_thumb = (point.x - center).abs() <= track.thumb / 2.0
                                && (point.y - track.y).abs() <= track.thumb / 2.0;
                            self.drag = Some(Drag {
                                offset: if on_thumb { point.x - center } else { 0.0 },
                                last_request: None,
                                last_x: on_thumb.then_some(point.x),
                            });
                            response.focus = FocusRequest::Focus;
                            response.capture = CaptureRequest::Capture;
                            if !on_thumb {
                                self.drag_to(point.x, &track, &mut response);
                            }
                        } else if !pressed {
                            self.drag_to(point.x, &track, &mut response);
                        }
                    }
                    if !pressed {
                        self.drag = None;
                        response.capture = CaptureRequest::Release;
                    }
                }
                WidgetEvent::Input(InputEvent::PointerMoved(point))
                    if self.enabled() && self.drag.is_some() =>
                {
                    response.handled = true;
                    if let Some(geometry) = context.geometry {
                        self.drag_to(
                            point.x,
                            &Track::new(geometry.content_bounds, self.properties.style),
                            &mut response,
                        );
                    }
                }
                WidgetEvent::Input(InputEvent::Key(key)) if self.enabled() && self.focused => {
                    if key.key == Key::Escape && self.drag.is_some() {
                        self.drag = None;
                        response.capture = CaptureRequest::Release;
                        response.handled = true;
                    } else if !key.modifiers.control
                        && !key.modifiers.alt
                        && !key.modifiers.super_key
                    {
                        let range = self.properties.range;
                        let next = match key.key {
                            Key::Home => Some(range.min()),
                            Key::End => Some(range.max()),
                            Key::ArrowRight | Key::ArrowUp => {
                                Some(range.shift(self.properties.value, true, 1))
                            }
                            Key::ArrowLeft | Key::ArrowDown => {
                                Some(range.shift(self.properties.value, false, 1))
                            }
                            Key::PageUp => Some(range.shift(self.properties.value, true, 10)),
                            Key::PageDown => Some(range.shift(self.properties.value, false, 10)),
                            _ => None,
                        };
                        if let Some(value) = next {
                            response.handled = true;
                            if key.pressed {
                                self.drag = None;
                                response.capture = CaptureRequest::Release;
                                self.request(value, &mut response);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if before != (self.focused, self.focus_visible, self.hovered, self.drag) {
            response.invalidation = Invalidation::Paint;
        }
        response
    }
}
