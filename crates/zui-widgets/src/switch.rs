//! Controlled boolean switch; labels compose through ordinary layout containers.
use crate::interaction::Interaction;
use core::time::Duration;
use zui_api::{
    action::{ActionId, WidgetAction},
    event::{EventContext, EventResponse, WidgetEvent},
    layout::LayoutError,
    view::ViewSpec,
    widget::{AnimationUpdate, Invalidation, MeasureContext, PaintContext, PrepareContext, Widget},
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, NodeId, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwitchChanged {
    pub checked: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwitchColors {
    pub off_track: Color,
    pub on_track: Color,
    pub off_thumb: Color,
    pub on_thumb: Color,
    pub border: Color,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchStyle {
    pub normal: SwitchColors,
    pub hovered: SwitchColors,
    pub pressed: SwitchColors,
    pub disabled: SwitchColors,
    pub focus_ring: Color,
    pub width: f32,
    pub height: f32,
    pub inset: f32,
}
impl SwitchStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        let normal = SwitchColors {
            off_track: theme.color(ColorRole::SurfaceRaised),
            on_track: theme.color(ColorRole::Accent),
            off_thumb: theme.color(ColorRole::Text),
            on_thumb: theme.color(ColorRole::OnAccent),
            border: theme.color(ColorRole::Border),
        };
        Self {
            normal,
            hovered: SwitchColors {
                border: theme.color(ColorRole::Accent),
                ..normal
            },
            pressed: SwitchColors {
                border: theme.color(ColorRole::Text),
                ..normal
            },
            disabled: SwitchColors {
                on_track: theme.color(ColorRole::SurfaceRaised),
                off_thumb: theme.color(ColorRole::TextMuted),
                on_thumb: theme.color(ColorRole::TextMuted),
                ..normal
            },
            focus_ring: theme.color(ColorRole::Text),
            width: 48.0,
            height: 26.0,
            inset: 3.0,
        }
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        if !self.width.is_finite()
            || !self.height.is_finite()
            || !self.inset.is_finite()
            || self.width < self.height
            || self.height < 4.0
            || self.inset < 1.0
            || self.inset * 2.0 >= self.height
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchView {
    pub checked: bool,
    pub disabled: bool,
    pub action: ActionId,
    pub style: SwitchStyle,
    /// None disables motion; Some must be nonzero. Only confirmed values animate.
    pub transition: Option<Duration>,
}
impl SwitchView {
    pub fn themed(theme: &Theme, checked: bool, action: ActionId) -> Self {
        Self {
            checked,
            disabled: false,
            action,
            style: SwitchStyle::from_theme(theme),
            transition: Some(Duration::from_millis(120)),
        }
    }
}
struct Motion {
    from: f64,
    start: Option<Duration>,
}
pub struct Switch {
    properties: SwitchView,
    interaction: Interaction,
    position: f64,
    motion: Option<Motion>,
}
impl ViewSpec for SwitchView {
    type Widget = Switch;
    fn build(&self) -> Switch {
        Switch {
            properties: *self,
            interaction: Default::default(),
            position: f64::from(self.checked),
            motion: None,
        }
    }
    fn update(&self, widget: &mut Switch) -> Invalidation {
        let old = widget.properties;
        if *self == old {
            return Invalidation::None;
        }
        if self.checked != old.checked || self.disabled || self.action != old.action {
            widget.interaction.cancel();
        }
        if self.disabled || self.transition.is_none() {
            widget.position = f64::from(self.checked);
            widget.motion = None;
        } else if self.checked != old.checked || self.transition != old.transition {
            widget.motion = (widget.position != f64::from(self.checked)).then_some(Motion {
                from: widget.position,
                start: None,
            });
        }
        widget.properties = *self;
        if self.style.width != old.style.width
            || self.style.height != old.style.height
            || self.style.inset != old.style.inset
            || self.transition != old.transition
        {
            Invalidation::Layout
        } else {
            Invalidation::Paint
        }
    }
}
impl Switch {
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        self.properties.style.validate(id)?;
        if self
            .properties
            .transition
            .is_some_and(|duration| duration.is_zero())
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        Ok(())
    }
    fn colors(&self) -> SwitchColors {
        let style = self.properties.style;
        if self.properties.disabled {
            style.disabled
        } else if self.interaction.space || (self.interaction.mouse && self.interaction.hovered) {
            style.pressed
        } else if self.interaction.hovered {
            style.hovered
        } else {
            style.normal
        }
    }
}
impl Widget for Switch {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.validate(context.id)?;
        Ok(Size::new(
            f64::from(
                context
                    .input
                    .known_width
                    .unwrap_or(self.properties.style.width),
            ),
            f64::from(
                context
                    .input
                    .known_height
                    .unwrap_or(self.properties.style.height),
            ),
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.validate(context.id)
    }
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        let Some(motion) = &mut self.motion else {
            return AnimationUpdate::default();
        };
        let duration = self.properties.transition.unwrap_or_default();
        let start = *motion.start.get_or_insert(elapsed);
        let passed = elapsed.saturating_sub(start);
        let t = if duration.is_zero() {
            1.0
        } else {
            (passed.as_secs_f64() / duration.as_secs_f64()).min(1.0)
        };
        let previous = self.position;
        let eased = t * t * (3.0 - 2.0 * t);
        self.position = motion.from + (f64::from(self.properties.checked) - motion.from) * eased;
        let next_frame = if t >= 1.0 {
            self.motion = None;
            None
        } else {
            elapsed.checked_add((duration - passed).min(Duration::from_millis(16)))
        };
        AnimationUpdate {
            invalidation: if previous != self.position {
                Invalidation::Paint
            } else {
                Invalidation::None
            },
            next_frame,
        }
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let bounds = context.content_bounds;
        let style = self.properties.style;
        let width = f64::from(style.width);
        let height = f64::from(style.height);
        let inset = f64::from(style.inset);
        let x = bounds.origin.x + ((bounds.size.width - width) / 2.0).max(0.0);
        let y = bounds.origin.y + ((bounds.size.height - height) / 2.0).max(0.0);
        let colors = self.colors();
        painter.with_clip(Path::rectangle(bounds), |painter| {
            painter.fill(
                Path::rounded_rectangle(Rect::new(x, y, width, height), height / 2.0)
                    .expect("validated switch geometry"),
                if self.properties.checked {
                    colors.on_track
                } else {
                    colors.off_track
                },
            );
            painter.stroke(
                Path::rounded_rectangle(
                    Rect::new(x + 0.5, y + 0.5, width - 1.0, height - 1.0),
                    (height - 1.0) / 2.0,
                )
                .expect("validated switch geometry"),
                colors.border,
                1.0,
            );
            let diameter = height - 2.0 * inset;
            painter.fill(
                Path::ellipse(Rect::new(
                    x + inset + (width - height) * self.position,
                    y + inset,
                    diameter,
                    diameter,
                ))
                .expect("validated switch geometry"),
                if self.properties.checked {
                    colors.on_thumb
                } else {
                    colors.off_thumb
                },
            );
        });
        if self.interaction.focused
            && self.enabled()
            && context.size.width > 4.0
            && context.size.height > 4.0
        {
            painter.stroke(
                Path::rectangle(Rect::new(
                    1.0,
                    1.0,
                    context.size.width - 2.0,
                    context.size.height - 2.0,
                )),
                style.focus_ring,
                2.0,
            );
        }
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        let (mut response, activated) =
            self.interaction
                .event(context, event, !self.properties.disabled, false);
        if activated {
            response.actions.push(WidgetAction::new(
                self.properties.action,
                SwitchChanged {
                    checked: !self.properties.checked,
                },
            ));
        }
        response
    }
}
