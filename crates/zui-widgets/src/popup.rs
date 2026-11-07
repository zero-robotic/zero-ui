//! Root-level overlays: positioning and dismissal without runtime dependencies.
use alloc::vec;
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Align, Dimension, LayoutError, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_types::{Color, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupPlacement {
    Center,
    /// Root-local anchor. Prefer below; flip above when it fits better.
    Anchor(Rect),
    /// Root-local position, e.g. a context-menu pointer location.
    Point(Point),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DismissReason {
    Escape,
    OutsidePointer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupDismissed {
    pub reason: DismissReason,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupView {
    pub placement: PopupPlacement,
    pub size: Size,
    pub margin: f64,
    /// None creates a passive overlay (e.g. Tooltip) which never takes focus or hits.
    pub dismiss: Option<ActionId>,
    pub close_on_outside: bool,
    pub backdrop: Color,
}
impl PopupView {
    pub fn new(placement: PopupPlacement, size: Size, dismiss: ActionId) -> Self {
        Self {
            placement,
            size,
            margin: 8.0,
            dismiss: Some(dismiss),
            close_on_outside: true,
            backdrop: Color::TRANSPARENT,
        }
    }
    /// Mount as a later sibling of the base content in a viewport-sized Stack.
    /// Anchor coordinates are local to that stack, not a scrolled descendant.
    pub fn build(self, mut content: View) -> View {
        content.style.width = Dimension::Length(self.size.width as f32);
        content.style.height = Dimension::Length(self.size.height as f32);
        content.style.max_width = Dimension::Percent(1.0);
        content.style.max_height = Dimension::Percent(1.0);
        content.style.shrink = 0.0;
        View::new(self)
            .styled(LayoutStyle {
                kind: LayoutKind::Stack,
                width: Dimension::Percent(1.0),
                height: Dimension::Percent(1.0),
                align: Align::Start,
                justify_items: Align::Start,
                ..Default::default()
            })
            .clipped(true)
            .with_children([content])
    }
    /// Validated root-local bounds; rejects invalid sizes, anchors and margins.
    pub fn bounds(self, viewport: Size) -> Result<Rect, zui_paint::RenderError> {
        let p = self;
        let valid_anchor = match p.placement {
            PopupPlacement::Center => true,
            PopupPlacement::Point(p) => p.is_finite(),
            PopupPlacement::Anchor(r) => r.is_valid(),
        };
        if !viewport.is_valid()
            || !p.size.is_valid()
            || p.size.width > f32::MAX as f64
            || p.size.height > f32::MAX as f64
            || !p.margin.is_finite()
            || p.margin < 0.0
            || !valid_anchor
        {
            return Err(zui_paint::RenderError::InvalidGeometry);
        }

        let width = self.size.width.min(viewport.width).max(0.0);
        let height = self.size.height.min(viewport.height).max(0.0);
        let max_x = (viewport.width - width).max(0.0);
        let max_y = (viewport.height - height).max(0.0);
        let (x, y) = match self.placement {
            PopupPlacement::Center => (max_x / 2.0, max_y / 2.0),
            PopupPlacement::Point(p) => (p.x, p.y),
            PopupPlacement::Anchor(r) => {
                let below = r.origin.y + r.size.height;
                let above = r.origin.y - height;
                (
                    r.origin.x,
                    if below + height > viewport.height && above >= 0.0 {
                        above
                    } else {
                        below
                    },
                )
            }
        };
        let mx = self.margin.min(max_x / 2.0);
        let my = self.margin.min(max_y / 2.0);
        Ok(Rect::new(
            x.clamp(mx, max_x - mx),
            y.clamp(my, max_y - my),
            width,
            height,
        ))
    }
}
pub struct Popup {
    properties: PopupView,
}
impl ViewSpec for PopupView {
    type Widget = Popup;
    fn build(&self) -> Popup {
        Popup { properties: *self }
    }
    fn update(&self, w: &mut Popup) -> Invalidation {
        if w.properties == *self {
            return Invalidation::None;
        }
        w.properties = *self;
        Invalidation::Layout
    }
}
impl Widget for Popup {
    fn focus_scope(&self) -> bool {
        self.properties.dismiss.is_some()
    }
    fn focusable(&self) -> bool {
        self.properties.dismiss.is_some()
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.properties
            .bounds(c.layout.bounds.size)
            .map_err(|_| LayoutError::InvalidStyle(c.id))?;
        Ok(())
    }
    fn child_viewport(&self, c: PaintContext) -> ChildViewport {
        ChildViewport {
            translation: self
                .properties
                .bounds(c.size)
                .expect("validated popup geometry")
                .origin,
            clip: Some(Rect::new(0.0, 0.0, c.size.width, c.size.height)),
        }
    }
    fn hit_test(&self, p: Point, size: Size) -> bool {
        self.properties.dismiss.is_some()
            && Rect::new(0.0, 0.0, size.width, size.height).contains(p)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        p.fill_rect(
            Rect::new(0.0, 0.0, c.size.width, c.size.height),
            self.properties.backdrop,
        );
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        let Some(action) = self.properties.dismiss else {
            return EventResponse::default();
        };
        let reason = match e {
            WidgetEvent::Input(InputEvent::Key(k))
                if k.pressed && k.key == Key::Escape && c.phase != EventPhase::Capture =>
            {
                Some(DismissReason::Escape)
            }
            WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
                if c.phase == EventPhase::Target
                    && self.properties.close_on_outside
                    && c.local_position.is_some_and(|p| {
                        self.properties
                            .bounds(c.geometry.map_or(Size::ZERO, |g| g.size))
                            .is_ok_and(|bounds| !bounds.contains(p))
                    }) =>
            {
                Some(DismissReason::OutsidePointer)
            }
            _ => None,
        };
        if let Some(reason) = reason {
            EventResponse {
                actions: vec![WidgetAction::new(action, PopupDismissed { reason })],
                handled: true,
                stop_propagation: true,
                ..Default::default()
            }
        } else {
            EventResponse::default()
        }
    }
}
