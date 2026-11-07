//! Passive progress display, sharing only paint/layout/animation contracts.
use core::time::Duration;
use zui_api::{
    layout::{AvailableSpace, LayoutError},
    view::ViewSpec,
    widget::{AnimationUpdate, Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::Painter;
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, NodeId, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProgressValue {
    /// Finite fraction in the inclusive range 0..=1.
    Determinate(f64),
    Indeterminate,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgressDirection {
    #[default]
    LeftToRight,
    RightToLeft,
    BottomToTop,
    TopToBottom,
}
impl ProgressDirection {
    fn horizontal(self) -> bool {
        matches!(self, Self::LeftToRight | Self::RightToLeft)
    }
    fn reversed(self) -> bool {
        matches!(self, Self::RightToLeft | Self::BottomToTop)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressAnimation {
    /// Full forward-and-back traversal time.
    pub cycle: Duration,
    pub frame_interval: Duration,
}
impl Default for ProgressAnimation {
    fn default() -> Self {
        Self {
            cycle: Duration::from_millis(1500),
            frame_interval: Duration::from_millis(33),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressBarStyle {
    pub track: Color,
    pub fill: Color,
    pub disabled_fill: Color,
    pub natural_length: f32,
    pub thickness: f32,
    /// Fraction of the track occupied by an indeterminate segment, in 0..1.
    pub segment_fraction: f64,
}
impl ProgressBarStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            track: theme.color(ColorRole::Border),
            fill: theme.color(ColorRole::Accent),
            disabled_fill: theme.color(ColorRole::TextMuted),
            natural_length: 200.0,
            thickness: 8.0,
            segment_fraction: 0.25,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressBarView {
    pub progress: ProgressValue,
    pub direction: ProgressDirection,
    pub disabled: bool,
    /// None displays a stationary centered segment, for reduced-motion interfaces.
    pub animation: Option<ProgressAnimation>,
    pub style: ProgressBarStyle,
}
impl ProgressBarView {
    pub fn themed(theme: &Theme, progress: ProgressValue) -> Self {
        Self {
            progress,
            direction: Default::default(),
            disabled: false,
            animation: Some(ProgressAnimation::default()),
            style: ProgressBarStyle::from_theme(theme),
        }
    }
    fn valid(&self) -> bool {
        let style = self.style;
        let value_valid = match self.progress {
            ProgressValue::Determinate(value) => value.is_finite() && (0.0..=1.0).contains(&value),
            ProgressValue::Indeterminate => true,
        };
        value_valid
            && style.natural_length.is_finite()
            && style.natural_length > 0.0
            && style.thickness.is_finite()
            && style.thickness > 0.0
            && style.segment_fraction.is_finite()
            && style.segment_fraction > 0.0
            && style.segment_fraction < 1.0
            && self.animation.is_none_or(|animation| {
                !animation.cycle.is_zero()
                    && !animation.frame_interval.is_zero()
                    && animation.frame_interval <= animation.cycle
            })
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        if self.valid() {
            Ok(())
        } else {
            Err(LayoutError::InvalidStyle(id))
        }
    }
}
pub struct ProgressBar {
    properties: ProgressBarView,
    start: Option<Duration>,
    position: f64,
    drawable: bool,
}
impl ViewSpec for ProgressBarView {
    type Widget = ProgressBar;
    fn build(&self) -> ProgressBar {
        ProgressBar {
            properties: *self,
            start: None,
            position: 0.0,
            drawable: false,
        }
    }
    fn update(&self, widget: &mut ProgressBar) -> Invalidation {
        let old = widget.properties;
        if *self == old {
            return Invalidation::None;
        }
        if self.progress != old.progress
            || self.animation != old.animation
            || self.disabled != old.disabled
        {
            widget.start = None;
            widget.position = 0.0;
        }
        widget.properties = *self;
        if !self.valid()
            || !old.valid()
            || self.direction != old.direction
            || self.style.natural_length != old.style.natural_length
            || self.style.thickness != old.style.thickness
        {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl Widget for ProgressBar {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.properties.validate(context.id)?;
        let style = self.properties.style;
        let horizontal = self.properties.direction.horizontal();
        let (width, height) = if horizontal {
            (style.natural_length, style.thickness)
        } else {
            (style.thickness, style.natural_length)
        };
        let preferred = |value: f32, available: AvailableSpace| match available {
            AvailableSpace::Definite(limit) => value.min(limit),
            AvailableSpace::MinContent => style.thickness.min(value),
            AvailableSpace::MaxContent => value,
        };
        Ok(Size::new(
            context
                .input
                .known_width
                .unwrap_or_else(|| preferred(width, context.input.available_width))
                as f64,
            context
                .input
                .known_height
                .unwrap_or_else(|| preferred(height, context.input.available_height))
                as f64,
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.properties.validate(context.id)?;
        self.drawable = context.content_size.width > 0.0 && context.content_size.height > 0.0;
        Ok(())
    }
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        let Some(animation) = self.properties.animation.filter(|_| {
            self.drawable
                && self.enabled()
                && self.properties.progress == ProgressValue::Indeterminate
                && self.properties.valid()
        }) else {
            self.start = None;
            return AnimationUpdate::default();
        };
        let start = *self.start.get_or_insert(elapsed);
        let phase = (elapsed.saturating_sub(start).as_nanos() % animation.cycle.as_nanos()) as f64
            / animation.cycle.as_nanos() as f64;
        let position = 1.0 - (2.0 * phase - 1.0).abs();
        let changed = position != self.position;
        self.position = position;
        AnimationUpdate {
            invalidation: if changed {
                Invalidation::Paint
            } else {
                Invalidation::None
            },
            next_frame: elapsed.checked_add(animation.frame_interval),
        }
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        if !self.properties.valid() {
            return;
        }
        let bounds = context.content_bounds;
        let style = self.properties.style;
        let horizontal = self.properties.direction.horizontal();
        let thickness = (style.thickness as f64).min(if horizontal {
            bounds.size.height
        } else {
            bounds.size.width
        });
        let track = if horizontal {
            Rect::new(
                bounds.origin.x,
                bounds.origin.y + (bounds.size.height - thickness) / 2.0,
                bounds.size.width,
                thickness,
            )
        } else {
            Rect::new(
                bounds.origin.x + (bounds.size.width - thickness) / 2.0,
                bounds.origin.y,
                thickness,
                bounds.size.height,
            )
        };
        if track.size.width <= 0.0 || track.size.height <= 0.0 {
            return;
        }
        painter.fill_rect(track, style.track);
        let (mut start, mut end) = match self.properties.progress {
            ProgressValue::Determinate(value) => (0.0, value),
            ProgressValue::Indeterminate => {
                let position = if self.properties.animation.is_none() || self.properties.disabled {
                    0.5
                } else {
                    self.position
                };
                let start = position * (1.0 - style.segment_fraction);
                (start, start + style.segment_fraction)
            }
        };
        if self.properties.direction.reversed() {
            (start, end) = (1.0 - end, 1.0 - start);
        }
        if end <= start {
            return;
        }
        let fill = if horizontal {
            Rect::new(
                track.origin.x + start * track.size.width,
                track.origin.y,
                (end - start) * track.size.width,
                track.size.height,
            )
        } else {
            Rect::new(
                track.origin.x,
                track.origin.y + start * track.size.height,
                track.size.width,
                (end - start) * track.size.height,
            )
        };
        painter.fill_rect(
            fill,
            if self.properties.disabled {
                style.disabled_fill
            } else {
                style.fill
            },
        );
    }
}
