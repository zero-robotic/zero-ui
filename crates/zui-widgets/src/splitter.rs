//! Two controlled panes. The application confirms each ratio request by rebuilding.
use crate::Container;
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Dimension, LayoutError, LayoutKind, LayoutStyle, Track},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, Rect};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitterChanged {
    pub ratio: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitterView {
    pub ratio: f64,
    pub axis: SplitAxis,
    pub action: ActionId,
    pub disabled: bool,
    pub min_ratio: f64,
    pub max_ratio: f64,
    pub handle_size: f32,
    pub color: Color,
    pub active_color: Color,
}
impl SplitterView {
    pub fn themed(theme: &Theme, ratio: f64, axis: SplitAxis, action: ActionId) -> Self {
        Self {
            ratio,
            axis,
            action,
            disabled: false,
            min_ratio: 0.1,
            max_ratio: 0.9,
            handle_size: 8.0,
            color: theme.color(ColorRole::Border),
            active_color: theme.color(ColorRole::Accent),
        }
    }
    pub fn build(self, first: View, second: View) -> View {
        let tracks = alloc::vec![
            Track::Fraction(self.ratio as f32),
            Track::Length(self.handle_size),
            Track::Fraction((1.0 - self.ratio) as f32)
        ];
        let mut style = LayoutStyle {
            kind: LayoutKind::Grid,
            ..Default::default()
        };
        if self.axis == SplitAxis::Horizontal {
            style.columns = tracks;
        } else {
            style.rows = tracks;
        }
        let pane = |view: View, key: &str| {
            crate::stack([view])
                .keyed(key)
                .clipped(true)
                .style(|s| {
                    s.min_width = Dimension::Length(0.0);
                    s.min_height = Dimension::Length(0.0);
                })
                .build()
        };
        View::new(self).styled(style).with_children(alloc::vec![
            pane(first, "first"),
            Container::empty().keyed("divider").build(),
            pane(second, "second")
        ])
    }
    fn valid(&self) -> bool {
        self.min_ratio.is_finite()
            && self.max_ratio.is_finite()
            && self.min_ratio >= 0.0
            && self.max_ratio <= 1.0
            && self.min_ratio <= self.max_ratio
            && self.ratio.is_finite()
            && (self.min_ratio..=self.max_ratio).contains(&self.ratio)
            && self.handle_size.is_finite()
            && self.handle_size >= 4.0
    }
    fn divider(&self, b: Rect) -> Rect {
        if self.axis == SplitAxis::Horizontal {
            Rect::new(
                b.origin.x + (b.size.width - self.handle_size as f64).max(0.0) * self.ratio,
                b.origin.y,
                (self.handle_size as f64).min(b.size.width),
                b.size.height,
            )
        } else {
            Rect::new(
                b.origin.x,
                b.origin.y + (b.size.height - self.handle_size as f64).max(0.0) * self.ratio,
                b.size.width,
                (self.handle_size as f64).min(b.size.height),
            )
        }
    }
}
pub struct Splitter {
    properties: SplitterView,
    drag: Option<f64>,
    focused: bool,
}
impl ViewSpec for SplitterView {
    type Widget = Splitter;
    fn build(&self) -> Splitter {
        Splitter {
            properties: *self,
            drag: None,
            focused: false,
        }
    }
    fn update(&self, w: &mut Splitter) -> Invalidation {
        if self.disabled
            || self.handle_size != w.properties.handle_size
            || self.axis != w.properties.axis
            || self.action != w.properties.action
            || self.min_ratio != w.properties.min_ratio
            || self.max_ratio != w.properties.max_ratio
        {
            w.drag = None;
        }
        let changed = *self != w.properties;
        w.properties = *self;
        if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}
impl Widget for Splitter {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        if self.properties.valid() {
            Ok(())
        } else {
            Err(LayoutError::InvalidStyle(c.id))
        }
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        p.fill_rect(
            self.properties.divider(c.content_bounds),
            if self.focused || self.drag.is_some() {
                self.properties.active_color
            } else {
                self.properties.color
            },
        );
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        let mut r = EventResponse::default();
        match e {
            WidgetEvent::FocusChanged(value) => {
                self.focused = *value;
                if !value {
                    self.drag = None;
                }
                r.invalidation = Invalidation::Paint;
            }
            WidgetEvent::PointerCaptureLost => {
                self.drag = None;
                r.invalidation = Invalidation::Paint;
            }
            _ => {}
        }
        if !self.enabled() || c.phase != EventPhase::Target {
            return r;
        }
        let Some(g) = c.geometry else {
            return r;
        };
        let b = g.content_bounds;
        let axis = |p: zui_types::Point| {
            if self.properties.axis == SplitAxis::Horizontal {
                p.x
            } else {
                p.y
            }
        };
        let total = if self.properties.axis == SplitAxis::Horizontal {
            b.size.width
        } else {
            b.size.height
        };
        let available = (total - self.properties.handle_size as f64).max(0.0);
        let mut request = None;
        match e {
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            }) => {
                if let Some(p) = c.local_position
                    && self.properties.divider(b).contains(p)
                {
                    self.drag = Some(axis(p) - axis(self.properties.divider(b).origin));
                    r.capture = CaptureRequest::Capture;
                    r.focus = FocusRequest::Focus;
                    r.handled = true;
                    r.invalidation = Invalidation::Paint;
                }
            }
            WidgetEvent::Input(InputEvent::PointerMoved { .. }) => {
                if let (Some(offset), Some(p)) = (self.drag, c.local_position)
                    && available > 0.0
                {
                    request = Some((axis(p) - axis(b.origin) - offset) / available);
                }
            }
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: false,
                ..
            }) if self.drag.take().is_some() => {
                r.capture = CaptureRequest::Release;
                r.handled = true;
                r.invalidation = Invalidation::Paint;
            }
            WidgetEvent::Input(InputEvent::Key(k))
                if self.focused
                    && k.pressed
                    && !k.modifiers.control
                    && !k.modifiers.alt
                    && !k.modifiers.super_key =>
            {
                let step = if k.modifiers.shift { 0.1 } else { 0.01 };
                request = match k.key {
                    Key::Home => Some(self.properties.min_ratio),
                    Key::End => Some(self.properties.max_ratio),
                    Key::ArrowLeft if self.properties.axis == SplitAxis::Horizontal => {
                        Some(self.properties.ratio - step)
                    }
                    Key::ArrowUp if self.properties.axis == SplitAxis::Vertical => {
                        Some(self.properties.ratio - step)
                    }
                    Key::ArrowRight if self.properties.axis == SplitAxis::Horizontal => {
                        Some(self.properties.ratio + step)
                    }
                    Key::ArrowDown if self.properties.axis == SplitAxis::Vertical => {
                        Some(self.properties.ratio + step)
                    }
                    Key::Escape => {
                        self.drag = None;
                        r.capture = CaptureRequest::Release;
                        r.handled = true;
                        None
                    }
                    _ => None,
                };
            }
            _ => {}
        }
        if let Some(ratio) = request {
            r.handled = true;
            let ratio = ratio.clamp(self.properties.min_ratio, self.properties.max_ratio);
            if ratio != self.properties.ratio {
                r.actions.push(WidgetAction::new(
                    self.properties.action,
                    SplitterChanged { ratio },
                ));
            }
        }
        r
    }
}
