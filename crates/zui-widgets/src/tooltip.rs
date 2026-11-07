//! Root-level, delayed hints. No input interception or focus ownership.
use crate::{Container, PopupPlacement, PopupView, TextView};
use alloc::{rc::Rc, string::String, vec::Vec};
use core::time::Duration;
use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutError, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Point, Rect, Size};
#[derive(Clone, Debug, PartialEq)]
pub struct TooltipAnchor {
    pub bounds: Rect,
    pub text: String,
}
#[derive(Clone)]
pub struct TooltipView {
    content: Rc<dyn Fn() -> View>,
    pub anchors: Vec<TooltipAnchor>,
    /// Optional application-provided keyboard hint index; does not move focus.
    pub focused: Option<usize>,
    pub delay: Duration,
    pub size: Size,
    pub theme: Theme,
    pub families: Vec<String>,
}
impl TooltipView {
    /// Install at the root. Anchors are in root-local coordinates; the content
    /// factory must be pure and retain stable keys when the tooltip changes.
    pub fn themed(
        theme: &Theme,
        families: Vec<String>,
        anchors: Vec<TooltipAnchor>,
        content: impl Fn() -> View + 'static,
    ) -> Self {
        Self {
            content: Rc::new(content),
            anchors,
            focused: None,
            delay: Duration::from_millis(500),
            size: Size::new(280.0, 72.0),
            theme: theme.clone(),
            families,
        }
    }
    pub fn build(self) -> View {
        View::new(self)
            .styled(LayoutStyle {
                kind: LayoutKind::Stack,
                width: Dimension::Percent(1.0),
                height: Dimension::Percent(1.0),
                ..Default::default()
            })
            .managed_children()
    }
}
pub struct Tooltip {
    properties: TooltipView,
    hovered: Option<usize>,
    since: Option<Duration>,
    visible: bool,
    suppressed: bool,
}
impl ViewSpec for TooltipView {
    type Widget = Tooltip;
    fn build(&self) -> Tooltip {
        Tooltip {
            properties: self.clone(),
            hovered: None,
            since: None,
            visible: false,
            suppressed: false,
        }
    }
    fn update(&self, w: &mut Tooltip) -> Invalidation {
        if self.anchors != w.properties.anchors
            || self.focused != w.properties.focused
            || self.delay != w.properties.delay
        {
            w.hovered = None;
            w.since = None;
            w.visible = false;
            w.suppressed = false;
        }
        w.properties = self.clone();
        Invalidation::Layout
    }
}
impl Tooltip {
    fn active(&self) -> Option<usize> {
        self.properties
            .focused
            .or(self.hovered)
            .filter(|i| *i < self.properties.anchors.len())
    }
}
impl Widget for Tooltip {
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        if !self.properties.size.is_valid()
            || self.properties.anchors.iter().any(|a| !a.bounds.is_valid())
            || self
                .properties
                .focused
                .is_some_and(|i| i >= self.properties.anchors.len())
        {
            return Err(LayoutError::InvalidStyle(c.id));
        }
        Ok(())
    }
    fn child_views(&self, _: PaintContext) -> Result<Vec<View>, LayoutError> {
        let mut children = alloc::vec![(self.properties.content)().keyed("content")];
        if self.visible
            && let Some(index) = self.active()
        {
            let anchor = &self.properties.anchors[index];
            let text = View::new(TextView::themed(
                &self.properties.theme,
                anchor.text.clone(),
                self.properties.families.clone(),
                TypeRole::Caption,
            ));
            let popup = PopupView {
                placement: PopupPlacement::Anchor(anchor.bounds),
                size: self.properties.size,
                margin: 8.0,
                dismiss: None,
                close_on_outside: false,
                backdrop: zui_types::Color::TRANSPARENT,
            };
            children.push(
                popup
                    .build(
                        Container::new(text)
                            .background(self.properties.theme.color(ColorRole::SurfaceRaised))
                            .style(|s| s.padding = Edges::all(12.0))
                            .build(),
                    )
                    .keyed("hint"),
            );
        }
        Ok(children)
    }
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        if self.active().is_none() || self.suppressed {
            self.since = None;
            return AnimationUpdate::default();
        }
        if self.visible {
            return AnimationUpdate::default();
        }
        let start = *self.since.get_or_insert(elapsed);
        let Some(due) = start.checked_add(self.properties.delay) else {
            return AnimationUpdate::default();
        };
        if elapsed >= due {
            self.visible = true;
            AnimationUpdate {
                invalidation: Invalidation::Layout,
                next_frame: None,
            }
        } else {
            AnimationUpdate {
                invalidation: Invalidation::None,
                next_frame: Some(due),
            }
        }
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        if c.phase == EventPhase::Bubble {
            return EventResponse::default();
        }
        let before = (self.hovered, self.visible, self.suppressed);
        match e {
            WidgetEvent::Input(InputEvent::PointerMoved(_)) => {
                let hovered = c.local_position.and_then(|p| {
                    self.properties
                        .anchors
                        .iter()
                        .position(|a| a.bounds.contains(p))
                });
                if hovered != self.hovered {
                    self.hovered = hovered;
                    self.since = None;
                    self.visible = false;
                    self.suppressed = false;
                }
            }
            WidgetEvent::Input(InputEvent::PointerLeft) => {
                self.hovered = None;
                self.since = None;
                self.visible = false;
                self.suppressed = false;
            }
            WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
            | WidgetEvent::Input(InputEvent::Scroll(_)) => {
                self.visible = false;
                self.suppressed = true;
            }
            WidgetEvent::Input(InputEvent::Key(k)) if k.pressed => {
                self.visible = false;
                self.suppressed = true;
            }
            _ => {}
        }
        EventResponse {
            handled: before.1
                && matches!(e,WidgetEvent::Input(InputEvent::Key(k)) if k.pressed && k.key==Key::Escape),
            invalidation: if before != (self.hovered, self.visible, self.suppressed) {
                Invalidation::Layout
            } else {
                Invalidation::None
            },
            ..Default::default()
        }
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
}
