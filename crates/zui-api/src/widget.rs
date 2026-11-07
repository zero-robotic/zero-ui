//! The runtime handles this contract without knowing concrete widget types.
use crate::layout::{LayoutError, MeasureInput};
use crate::text::TextService;
use core::any::Any;
use core::time::Duration;
use zui_paint::Painter;
use zui_types::{NodeId, Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Mounted,
    Unmounted,
}

/// Layout invalidation also implies paint invalidation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Invalidation {
    #[default]
    None,
    Paint,
    Layout,
}
/// Animation work computed from application-supplied monotonic elapsed time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationUpdate {
    pub invalidation: Invalidation,
    /// Absolute elapsed time, strictly later than the time passed to animate.
    /// None withdraws this widget's request; no continuous frame loop is implied.
    pub next_frame: Option<Duration>,
}

pub struct MeasureContext<'a> {
    pub id: NodeId,
    pub input: MeasureInput,
    pub text: &'a mut dyn TextService,
}

/// Called after layout validation, even for leaves whose definite dimensions
/// let the engine skip measurement. Failure prevents publication of the layout.
pub struct PrepareContext<'a> {
    pub id: NodeId,
    /// Validated final layout, including reachable content extent.
    pub layout: crate::layout::LayoutBox,
    pub content_size: Size,
    pub text: &'a mut dyn TextService,
}

#[derive(Clone, Copy, Debug)]
pub struct PaintContext {
    pub id: NodeId,
    pub size: Size,
    pub content_bounds: Rect,
}

/// Opaque association supplied by a parent, echoed with layout feedback. It is
/// independent of the stable reconciliation key and never interpreted by core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutTag {
    pub revision: u64,
    pub value: u64,
}

/// Snapshot of a direct child's validated layout, before presentation transforms.
/// The key is scoped to this parent. No runtime node or concrete widget is exposed.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildLayout {
    pub id: NodeId,
    pub key: Option<crate::view::Key>,
    pub tag: Option<LayoutTag>,
    pub layout: crate::layout::LayoutBox,
}

/// Presentation of descendants in the parent's local border-box coordinates.
/// Does not change layout, the parent's own paint, or its own hit area.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChildViewport {
    /// Translation applied after each child's layout position. Scrolling down
    /// normally uses a negative y translation. Must be finite.
    pub translation: Point,
    /// Optional stationary clip, applied before translating descendants.
    pub clip: Option<Rect>,
}

/// Optional text-input capability. Cursor area is in the widget's border-box space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputState {
    /// Change when an existing composition must be cancelled and restarted.
    pub generation: u64,
    pub cursor_area: Rect,
}

/// Persistent widget state lives in the implementing object. Layout style and
/// child ownership live in the runtime. All drawing/hits use local coordinates.
/// Services are borrowed only during callbacks, never retained.
pub trait Widget: Any {
    /// Pure description query, called after valid layout only for views opting
    /// into widget-owned children. May run repeatedly while geometry converges.
    /// Return the complete desired set, using stable keys to retain local state.
    /// A change affecting this result must request at least Paint invalidation.
    /// Do not consume requests here: validation can fail and retry this query.
    fn child_views(
        &self,
        _context: PaintContext,
    ) -> Result<alloc::vec::Vec<crate::view::View>, LayoutError> {
        Ok(alloc::vec::Vec::new())
    }
    /// Feedback for widget-owned children after all widgets have prepared and a
    /// complete layout has been validated. Children are in current sibling order.
    /// Called only when layout actually runs, not for paint-only refreshes.
    /// May update local measurement/index state, never topology directly. Report
    /// Layout if measurement changes; Paint if only descriptions/presentation change.
    /// This callback may repeat during bounded convergence; avoid external effects.
    fn children_prepared(
        &mut self,
        _context: PaintContext,
        _children: &[ChildLayout],
    ) -> Result<Invalidation, LayoutError> {
        Ok(Invalidation::None)
    }
    /// A pure geometry query. Changes require at least Paint invalidation.
    /// The runtime applies this to paint, hits, event coordinates and IME areas.
    fn child_viewport(&self, _context: PaintContext) -> ChildViewport {
        ChildViewport::default()
    }
    /// Reveal a descendant area expressed in this widget's current local
    /// presentation coordinates. May change only presentation (e.g. offset),
    /// never layout or topology. Return true when paint must be invalidated.
    fn reveal_child(&mut self, _context: PaintContext, _area: Rect) -> bool {
        false
    }
    fn animate(&mut self, _elapsed: Duration) -> AnimationUpdate {
        AnimationUpdate::default()
    }
    fn text_input(&self, _context: PaintContext) -> Option<TextInputState> {
        None
    }
    /// Disabled controls may remain hittable to block click-through, but cannot
    /// own focus or pointer capture. Visual/event behavior belongs to the control.
    fn enabled(&self) -> bool {
        true
    }
    /// Active focus scope: the last mounted scope confines traversal and input.
    /// Core enters it after layout and restores the previous eligible focus on removal.
    /// Scope owners should be focusable as a fallback when they have no children.
    fn focus_scope(&self) -> bool {
        false
    }
    fn focusable(&self) -> bool {
        false
    }
    /// Called for unhandled Tab/Shift+Tab without Control/Alt/Super, nearest first.
    /// May emit an application action to realize a missing destination and handle
    /// traversal. No geometry or mutable tree is exposed; focus remains unchanged
    /// until an explicit request. The candidate is a snapshot before callbacks.
    fn navigate_focus(
        &mut self,
        _navigation: crate::event::FocusNavigation,
    ) -> crate::event::FocusNavigationResponse {
        crate::event::FocusNavigationResponse::default()
    }
    /// Intercept default Tab entering from outside, after outgoing callbacks
    /// decline. Outermost new ancestor is asked first. Handled leaves focus
    /// unchanged; actions can arrange realization and a later focus request.
    fn enter_focus(
        &mut self,
        _entry: crate::event::FocusEntry,
    ) -> crate::event::FocusNavigationResponse {
        crate::event::FocusNavigationResponse::default()
    }

    fn event(
        &mut self,
        _context: crate::event::EventContext,
        _event: &crate::event::WidgetEvent,
    ) -> crate::event::EventResponse {
        Default::default()
    }
    fn lifecycle(&mut self, _id: NodeId, _event: Lifecycle) {}
    fn event_with_services(
        &mut self,
        context: crate::event::EventContext,
        event: &crate::event::WidgetEvent,
        _services: &mut crate::event::EventServices<'_>,
    ) -> crate::event::EventResponse {
        self.event(context, event)
    }
    /// Preferred layout box width/height ratio when one axis is automatic.
    /// Must be finite and positive; changes require Layout invalidation.
    fn preferred_aspect_ratio(&self) -> Option<f32> {
        None
    }
    fn measure(&mut self, _context: MeasureContext) -> Result<Size, LayoutError> {
        Ok(Size::ZERO)
    }
    fn prepare(&mut self, _context: PrepareContext) -> Result<(), LayoutError> {
        Ok(())
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>);
    /// Draw after all descendants, in this widget's local space. Ancestor and
    /// node clips still apply; the widget's own child viewport does not.
    fn paint_foreground(&self, _context: PaintContext, _painter: &mut Painter<'_>) {}
    /// Hit regions belonging to foreground decorations before testing children.
    /// Return false for visual-only decorations. Must agree with foreground paint.
    fn hit_test_foreground(&self, _point: Point, _context: PaintContext) -> bool {
        false
    }
    fn hit_test(&self, point: Point, size: Size) -> bool {
        Rect::new(0.0, 0.0, size.width, size.height).contains(point)
    }
}
