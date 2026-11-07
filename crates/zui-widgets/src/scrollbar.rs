//! Overlay scrollbar geometry. No runtime, input routing or layout dependency.
use crate::axis::Axis;
use zui_types::{Color, Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarStyle {
    pub width: f64,
    pub min_thumb: f64,
    pub track: Color,
    pub thumb: Color,
    pub dragging: Color,
    pub disabled: Color,
}
impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            width: 8.0,
            min_thumb: 18.0,
            track: Color::rgba(15, 35, 50, 220),
            thumb: Color::rgb(90, 130, 150),
            dragging: Color::rgb(41, 210, 230),
            disabled: Color::rgb(65, 80, 90),
        }
    }
}
impl ScrollbarStyle {
    pub(super) fn valid(self) -> bool {
        self.width.is_finite()
            && self.width > 0.0
            && self.min_thumb.is_finite()
            && self.min_thumb > 0.0
    }
    pub(super) fn geometry(
        self,
        axis: Axis,
        content: Rect,
        offset: f64,
        max: f64,
        shorten: f64,
    ) -> Option<BarGeometry> {
        if !self.valid() || max <= 0.0 || content.size.width <= 0.0 || content.size.height <= 0.0 {
            return None;
        }
        let page = axis.length(content.size);
        let length = (page - shorten).max(0.0);
        if length <= 0.0 {
            return None;
        }
        let cross = axis.other();
        let width = self.width.min(cross.length(content.size));
        let thumb_length = (length * (page / (page + max)))
            .max(self.min_thumb)
            .min(length);
        let start = axis.value(content.origin);
        let cross_start = cross.value(content.origin) + cross.length(content.size) - width;
        let track = axis.rect(start, cross_start, length, width);
        let travel = length - thumb_length;
        let thumb = axis.rect(
            start + offset / max * travel,
            cross_start,
            thumb_length,
            width,
        );
        Some(BarGeometry {
            axis,
            track,
            thumb,
            travel,
        })
    }
}
#[derive(Clone, Copy)]
pub(super) struct BarGeometry {
    pub axis: Axis,
    pub track: Rect,
    pub thumb: Rect,
    pub travel: f64,
}
impl BarGeometry {
    pub fn grab_fraction(self, pointer: Point) -> f64 {
        (self.axis.value(pointer) - self.axis.value(self.thumb.origin))
            / self.axis.length(self.thumb.size)
    }
    pub fn drag_offset(self, pointer: Point, grab_fraction: f64, max: f64) -> Option<f64> {
        if self.travel <= 0.0 || !pointer.is_finite() {
            return None;
        }
        Some(
            ((self.axis.value(pointer)
                - self.axis.value(self.track.origin)
                - grab_fraction * self.axis.length(self.thumb.size))
                / self.travel)
                .clamp(0.0, 1.0)
                * max,
        )
    }
}
