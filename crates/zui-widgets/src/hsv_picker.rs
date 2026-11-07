//! Portable HSV plane and hue strip. All values and actions are independent of backends.
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::LayoutError,
    view::ViewSpec,
    widget::*,
};
use zui_paint::{Painter, Path};
use zui_types::{Color, Point, Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsvColor {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
}
impl HsvColor {
    pub fn new(hue: f32, saturation: f32, value: f32) -> Option<Self> {
        (hue.is_finite()
            && (0.0..=360.0).contains(&hue)
            && saturation.is_finite()
            && (0.0..=1.0).contains(&saturation)
            && value.is_finite()
            && (0.0..=1.0).contains(&value))
        .then_some(Self {
            hue: if hue == 360.0 { 0.0 } else { hue },
            saturation,
            value,
        })
    }
    pub fn from_color(color: Color) -> Self {
        let (r, g, b) = (
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let mut hue = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / delta)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        if hue < 0.0 {
            hue += 360.0;
        }
        Self {
            hue,
            saturation: if max == 0.0 { 0.0 } else { delta / max },
            value: max,
        }
    }
    pub fn to_color(self, alpha: u8) -> Option<Color> {
        let hsv = Self::new(self.hue, self.saturation, self.value)?;
        let h = hsv.hue / 60.0;
        let c = hsv.value * hsv.saturation;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        let m = hsv.value - c;
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let byte = |v: f32| ((v + m) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        Some(Color::rgba(byte(r), byte(g), byte(b), alpha))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsvChanged {
    pub hsv: HsvColor,
    pub color: Color,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsvPickerView {
    pub hsv: HsvColor,
    pub alpha: u8,
    pub disabled: bool,
    pub action: ActionId,
}
pub struct HsvPicker {
    properties: HsvPickerView,
    drag: Option<bool>,
    focused: bool,
}
impl ViewSpec for HsvPickerView {
    type Widget = HsvPicker;
    fn build(&self) -> HsvPicker {
        HsvPicker {
            properties: *self,
            drag: None,
            focused: false,
        }
    }
    fn update(&self, w: &mut HsvPicker) -> Invalidation {
        if self.disabled || self.action != w.properties.action {
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
impl HsvPicker {
    fn emit(&self, hsv: HsvColor) -> EventResponse {
        let mut r = EventResponse {
            handled: true,
            ..Default::default()
        };
        if let Some(color) = hsv.to_color(self.properties.alpha) {
            r.actions.push(WidgetAction::new(
                self.properties.action,
                HsvChanged { hsv, color },
            ));
            r.actions.push(WidgetAction::new(
                self.properties.action,
                crate::ColorChanged { color },
            ));
        }
        r
    }
    fn point(&self, p: Point, size: Size, hue: bool) -> EventResponse {
        let mut hsv = self.properties.hsv;
        if hue {
            hsv.hue = ((p.x / size.width.max(1.0)).clamp(0.0, 1.0) * 359.99) as f32;
        } else {
            hsv.saturation = (p.x / size.width.max(1.0)).clamp(0.0, 1.0) as f32;
            hsv.value = (1.0 - p.y / (size.height - 28.0).max(1.0)).clamp(0.0, 1.0) as f32;
        }
        self.emit(hsv)
    }
}
impl Widget for HsvPicker {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, _: MeasureContext) -> Result<Size, LayoutError> {
        Ok(Size::new(224.0, 156.0))
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        if self
            .properties
            .hsv
            .to_color(self.properties.alpha)
            .is_none()
        {
            Err(LayoutError::InvalidStyle(c.id))
        } else {
            Ok(())
        }
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        let b = c.content_bounds;
        let plane = (b.size.height - 28.0).max(0.0);
        p.with_clip(Path::rectangle(b), |p| {
            // Fixed bounded tessellation, shared by CPU, GPU and no_std backends.
            for y in 0..24 {
                for x in 0..32 {
                    let hsv = HsvColor {
                        hue: self.properties.hsv.hue,
                        saturation: x as f32 / 31.0,
                        value: 1.0 - y as f32 / 23.0,
                    };
                    if let Some(color) = hsv.to_color(255) {
                        p.fill_rect(
                            Rect::new(
                                b.origin.x + x as f64 * b.size.width / 32.0,
                                b.origin.y + y as f64 * plane / 24.0,
                                b.size.width / 32.0 + 0.1,
                                plane / 24.0 + 0.1,
                            ),
                            color,
                        );
                    }
                }
            }
            for x in 0..72 {
                let hsv = HsvColor {
                    hue: x as f32 * 5.0,
                    saturation: 1.0,
                    value: 1.0,
                };
                p.fill_rect(
                    Rect::new(
                        b.origin.x + x as f64 * b.size.width / 72.0,
                        b.origin.y + plane + 8.0,
                        b.size.width / 72.0 + 0.1,
                        20.0,
                    ),
                    hsv.to_color(255).unwrap_or(Color::BLACK),
                );
            }
            let x = b.origin.x + self.properties.hsv.saturation as f64 * b.size.width;
            let y = b.origin.y + (1.0 - self.properties.hsv.value as f64) * plane;
            p.stroke(
                Path::rectangle(Rect::new(x - 3.0, y - 3.0, 6.0, 6.0)),
                Color::WHITE,
                1.0,
            );
            p.stroke(
                Path::rectangle(Rect::new(x - 4.0, y - 4.0, 8.0, 8.0)),
                Color::BLACK,
                1.0,
            );
            let hue_x = b.origin.x + self.properties.hsv.hue as f64 / 360.0 * b.size.width;
            p.stroke(
                Path::rectangle(Rect::new(hue_x - 2.0, b.origin.y + plane + 8.0, 4.0, 20.0)),
                Color::WHITE,
                1.0,
            );
            if self.focused {
                p.stroke(
                    Path::rectangle(Rect::new(
                        b.origin.x + 1.0,
                        b.origin.y + 1.0,
                        (b.size.width - 2.0).max(0.0),
                        (b.size.height - 2.0).max(0.0),
                    )),
                    Color::WHITE,
                    1.0,
                );
            }
            if self.properties.disabled {
                p.fill_rect(b, Color::rgba(100, 100, 100, 120));
            }
        });
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        match e {
            WidgetEvent::FocusChanged(f) => {
                self.focused = *f;
                if !f {
                    self.drag = None;
                }
                return EventResponse {
                    invalidation: Invalidation::Paint,
                    ..Default::default()
                };
            }
            WidgetEvent::PointerCaptureLost => {
                self.drag = None;
                return EventResponse::default();
            }
            _ => {}
        }
        if !self.enabled() || c.phase != EventPhase::Target {
            return EventResponse::default();
        }
        if let WidgetEvent::Input(InputEvent::Key(k)) = e
            && k.pressed
        {
            if k.key == Key::Escape {
                self.drag = None;
                return EventResponse {
                    handled: true,
                    capture: CaptureRequest::Release,
                    ..Default::default()
                };
            }
            if k.modifiers.control || k.modifiers.alt || k.modifiers.super_key {
                return EventResponse::default();
            }
            let mut hsv = self.properties.hsv;
            let step = if k.modifiers.shift { 0.1 } else { 0.01 };
            match k.key {
                Key::ArrowLeft => hsv.saturation = (hsv.saturation - step).max(0.0),
                Key::ArrowRight => hsv.saturation = (hsv.saturation + step).min(1.0),
                Key::ArrowDown => hsv.value = (hsv.value - step).max(0.0),
                Key::ArrowUp => hsv.value = (hsv.value + step).min(1.0),
                Key::PageDown => hsv.hue = (hsv.hue + 355.0) % 360.0,
                Key::PageUp => hsv.hue = (hsv.hue + 5.0) % 360.0,
                _ => return EventResponse::default(),
            }
            return self.emit(hsv);
        }
        if let (Some(pos), Some(g)) = (c.local_position, c.geometry) {
            let local = Point::new(
                pos.x - g.content_bounds.origin.x,
                pos.y - g.content_bounds.origin.y,
            );
            match e {
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                }) => {
                    let hue = local.y >= g.content_bounds.size.height - 20.0;
                    if !hue && local.y > g.content_bounds.size.height - 28.0 {
                        return EventResponse::default();
                    }
                    self.drag = Some(hue);
                    let mut r = self.point(local, g.content_bounds.size, hue);
                    r.focus = FocusRequest::Focus;
                    r.capture = CaptureRequest::Capture;
                    return r;
                }
                WidgetEvent::Input(InputEvent::PointerMoved(_)) => {
                    if let Some(hue) = self.drag {
                        return self.point(local, g.content_bounds.size, hue);
                    }
                }
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }) => {
                    if let Some(hue) = self.drag.take() {
                        let mut r = self.point(local, g.content_bounds.size, hue);
                        r.capture = CaptureRequest::Release;
                        return r;
                    }
                }
                _ => {}
            }
        }
        EventResponse::default()
    }
}
