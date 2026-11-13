//! Size-indexed realization with local coordinates and shared scroll behavior.
mod measured;
mod navigation;
pub use navigation::VirtualListNavigate;
mod sizes;
use super::*;
use crate::virtualization::{Alignment, ExtentIndex, Window};
use alloc::{rc::Rc, vec::Vec};
use sizes::RowSizes;
use zui_api::widget::{ChildLayout, LayoutTag};

#[derive(Clone, Copy, Debug, PartialEq)]
struct IndexRequest {
    index: usize,
    alignment: Alignment,
    revision: u64,
}
#[derive(Clone)]
struct ListProperties {
    scroll: Properties,
    sizes: RowSizes,
    preserve_anchor: bool,
    gap: f64,
    index_limits: crate::virtualization::Limits,
    overscan: usize,
    max_realized: usize,
    request: Option<IndexRequest>,
    navigation_action: Option<ActionId>,
    navigable_rows: Option<Rc<[usize]>>,
    factory: Rc<dyn Fn(usize) -> View>,
}
/// Vertical list with fixed, supplied or automatically measured row heights.
/// Factories must return views with stable business
/// keys and may run more than once during layout convergence. Removed rows lose
/// local widget state; keep persistent editing/business state in the data model.
pub struct VirtualListView {
    properties: ListProperties,
    style: LayoutStyle,
    key: Option<ViewKey>,
}
impl VirtualListView {
    pub fn new(count: usize, row_height: f64, factory: impl Fn(usize) -> View + 'static) -> Self {
        Self::with_sizes(
            RowSizes::Fixed {
                count,
                height: row_height,
            },
            factory,
        )
    }
    /// Immutable heights supplied by the application, not measured automatically.
    /// Reuse the same Rc across rebuilds to avoid rebuilding the aggregate index.
    pub fn variable(row_heights: Rc<[f64]>, factory: impl Fn(usize) -> View + 'static) -> Self {
        Self::with_sizes(RowSizes::Variable(row_heights), factory)
    }
    /// Measure realized rows at the actual content width. Unseen rows use the
    /// estimate. Change data_revision when content, identity or order changes.
    pub fn measured(
        count: usize,
        estimated_height: f64,
        data_revision: u64,
        factory: impl Fn(usize) -> View + 'static,
    ) -> Self {
        let mut view = Self::with_sizes(
            RowSizes::Measured {
                count,
                estimate: estimated_height,
                revision: data_revision,
            },
            factory,
        );
        view.properties.preserve_anchor = true;
        view
    }
    fn with_sizes(sizes: RowSizes, factory: impl Fn(usize) -> View + 'static) -> Self {
        Self {
            properties: ListProperties {
                scroll: Properties::default(),
                sizes,
                preserve_anchor: false,
                gap: 0.0,
                index_limits: Default::default(),
                overscan: 2,
                max_realized: 512,
                request: None,
                navigation_action: None,
                navigable_rows: None,
                factory: Rc::new(factory),
            },
            style: LayoutStyle::default(),
            key: None,
        }
    }
    /// Preserve the first visible logical index and its relative position when
    /// heights/gaps change. Requires unchanged item order/identity. Explicit new
    /// scroll requests take precedence; structural edits should leave this false.
    pub fn preserve_visible_anchor(mut self, preserve: bool) -> Self {
        self.properties.preserve_anchor = preserve;
        self
    }
    /// Limits for the independent dimension index, separate from UI node limits.
    pub fn index_limits(mut self, limits: crate::virtualization::Limits) -> Self {
        self.properties.index_limits = limits;
        self
    }
    pub fn style(mut self, configure: impl FnOnce(&mut LayoutStyle)) -> Self {
        configure(&mut self.style);
        self
    }
    pub fn keyed(mut self, key: impl Into<ViewKey>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn gap(mut self, gap: f64) -> Self {
        self.properties.gap = gap;
        self
    }
    pub fn overscan(mut self, count: usize) -> Self {
        self.properties.overscan = count;
        self
    }
    /// Reject before calling row factories if a viewport would exceed this count.
    pub fn max_realized(mut self, count: usize) -> Self {
        self.properties.max_realized = count;
        self
    }
    pub fn scrollbar(mut self, style: Option<ScrollbarStyle>) -> Self {
        self.properties.scroll.scrollbar = style;
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.properties.scroll.background = Some(color);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.properties.scroll.disabled = disabled;
        self
    }
    pub fn line_step(mut self, step: f64) -> Self {
        self.properties.scroll.line_step = step;
        self
    }
    pub fn on_scroll(mut self, action: ActionId) -> Self {
        self.properties.scroll.action = Some(action);
        self
    }
    /// Opt in to logical row traversal. The application must consume the action,
    /// realize the requested row and focus its first/last eligible descendant.
    pub fn on_navigate(mut self, action: ActionId) -> Self {
        self.properties.navigation_action = Some(action);
        self
    }
    /// Sorted unique logical indices whose rows contain eligible focus targets.
    /// Reuse the Rc across rebuilds. Empty means no navigable rows; omission
    /// assumes all rows. Keep this metadata consistent with row controls/data.
    pub fn navigable_rows(mut self, rows: Rc<[usize]>) -> Self {
        self.properties.navigable_rows = Some(rows);
        self
    }
    pub fn scroll_to(mut self, offset: f64, revision: u64) -> Self {
        self.properties.request = None;
        self.properties.scroll.request = Some(ScrollRequest {
            offset: Point::new(0.0, offset),
            revision,
        });
        self
    }
    pub fn scroll_to_index(mut self, index: usize, alignment: Alignment, revision: u64) -> Self {
        self.properties.scroll.request = None;
        self.properties.request = Some(IndexRequest {
            index,
            alignment,
            revision,
        });
        self
    }
    pub fn build(mut self) -> View {
        self.style.kind = LayoutKind::Column;
        self.style.align = Align::Start;
        self.style.justify = Justify::Start;
        self.style.wrap = false;
        self.style.gap = Size::new(0.0, self.properties.gap);
        self.style.rows.clear();
        self.style.columns.clear();
        self.style.overflow_x = Overflow::Clip;
        self.style.overflow_y = Overflow::Scroll;
        let mut view = View::new(self.properties)
            .styled(self.style)
            .managed_children();
        view.key = self.key;
        view
    }
}
impl From<VirtualListView> for View {
    fn from(view: VirtualListView) -> Self {
        view.build()
    }
}
/// Only realized rows are present in the tree. Logical offset and range remain f64.
pub struct VirtualList {
    properties: ListProperties,
    scroll: Scroll,
    index: Option<ExtentIndex>,
    index_dirty: bool,
    measurement_context: Option<(f64, u64)>,
    measurement_generation: u64,
    measurement_jump: Option<IndexRequest>,
    origin: f64,
    pending_index: bool,
    navigation: navigation::NavigationState,
}
impl VirtualList {
    pub fn offset(&self) -> f64 {
        self.scroll.offset().y
    }
    pub fn max_offset(&self) -> f64 {
        self.scroll.max_offset().y
    }
    pub fn allocated_index_bytes(&self) -> usize {
        self.index
            .as_ref()
            .map_or(0, ExtentIndex::allocated_index_bytes)
    }
    /// Explicit maintenance; never invoked on every frame. The runtime owner must
    /// request Layout invalidation when calling this through Tree::update_widget.
    /// No measured heights are evicted. Returns released index-buffer bytes.
    pub fn compact_index(
        &mut self,
        max_scratch_bytes: usize,
    ) -> Result<usize, crate::virtualization::Error> {
        match &mut self.index {
            Some(index) => index.compact(max_scratch_bytes),
            None => Ok(0),
        }
    }
    fn window(&self, id: zui_types::NodeId) -> Result<Window, LayoutError> {
        let index = self.index.as_ref().ok_or(LayoutError::InvalidStyle(id))?;
        let window = index
            .window(
                self.offset(),
                self.scroll.page.height,
                self.properties.overscan,
            )
            .map_err(|_| LayoutError::InvalidStyle(id))?;
        if window.items.len() > self.properties.max_realized {
            return Err(LayoutError::BudgetExceeded);
        }
        Ok(window)
    }
}
impl ViewSpec for ListProperties {
    type Widget = VirtualList;
    fn build(&self) -> VirtualList {
        VirtualList {
            properties: self.clone(),
            scroll: self.scroll.build(),
            index: None,
            index_dirty: true,
            measurement_context: None,
            measurement_generation: 0,
            measurement_jump: None,
            origin: 0.0,
            pending_index: true,
            navigation: Default::default(),
        }
    }
    fn update(&self, widget: &mut VirtualList) -> Invalidation {
        let old = &widget.properties;
        let sizes_changed = !self.sizes.same(&old.sizes)
            || self.gap != old.gap
            || self.index_limits != old.index_limits;
        widget.index_dirty |= sizes_changed;
        if sizes_changed {
            widget.measurement_jump = None;
        }
        let geometry = !navigation::same_rows(&self.navigable_rows, &old.navigable_rows)
            || self.navigation_action != old.navigation_action
            || sizes_changed
            || self.overscan != old.overscan
            || self.max_realized != old.max_realized
            || self.request != old.request;
        let factory = !Rc::ptr_eq(&self.factory, &old.factory);
        widget.pending_index |= self.request != old.request;
        let scroll = self.scroll.update(&mut widget.scroll);
        widget.properties = self.clone();
        scroll.max(if geometry {
            Invalidation::Layout
        } else if factory {
            Invalidation::Paint
        } else {
            Invalidation::None
        })
    }
}
impl Widget for VirtualList {
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        let id = context.id;
        if self.properties.sizes.measured() {
            let signature = (context.content_size.width, context.text.font_epoch());
            self.index_dirty |= self.measurement_context != Some(signature);
            self.measurement_context = Some(signature);
        }
        let replacement = if self.index_dirty {
            Some(
                self.properties
                    .sizes
                    .build_index(self.properties.gap, self.properties.index_limits)
                    .map_err(|error| match error {
                        crate::virtualization::Error::BudgetExceeded
                        | crate::virtualization::Error::AllocationFailed => {
                            LayoutError::BudgetExceeded
                        }
                        _ => LayoutError::InvalidStyle(id),
                    })?,
            )
        } else {
            None
        };
        let index = replacement
            .as_ref()
            .or(self.index.as_ref())
            .ok_or(LayoutError::InvalidStyle(id))?;
        self.navigation
            .validate(&self.properties.navigable_rows, index.len(), id)?;
        let page = context.content_size.height;
        let requested = self
            .pending_index
            .then_some(self.properties.request)
            .flatten();
        let pixel_request = self.scroll.pending && self.properties.scroll.request.is_some();
        let anchor = if replacement.is_some()
            && self.properties.preserve_anchor
            && requested.is_none()
            && !pixel_request
        {
            self.index
                .as_ref()
                .map(|old| old.anchor(self.offset(), self.scroll.page.height))
                .transpose()
                .map_err(|_| LayoutError::InvalidStyle(id))?
                .flatten()
                .filter(|anchor| anchor.index < index.len())
        } else {
            None
        };
        let target = if let Some(request) = requested {
            Some(
                index
                    .offset_for(request.index, self.offset(), page, request.alignment)
                    .map_err(|_| LayoutError::InvalidStyle(id))?,
            )
        } else if let Some(anchor) = anchor {
            Some(
                index
                    .offset_for_anchor(anchor, page)
                    .map_err(|_| LayoutError::InvalidStyle(id))?,
            )
        } else {
            None
        };
        let max = (index.total_extent() - page).max(0.0);
        self.scroll.prepare_range(context, Point::new(0.0, max))?;
        if let Some(offset) = target {
            self.scroll.offset.y = offset;
        }
        if let Some(index) = replacement {
            if self.properties.sizes.measured() {
                self.measurement_generation = self
                    .measurement_generation
                    .checked_add(1)
                    .ok_or(LayoutError::BudgetExceeded)?;
            }
            self.index = Some(index);
        }
        if self.properties.sizes.measured() {
            if requested.is_some() {
                self.measurement_jump = requested;
            }
            if pixel_request {
                self.measurement_jump = None;
            }
        }
        self.index_dirty = false;
        self.pending_index = false;
        self.origin = self.window(id)?.leading_extent;
        Ok(())
    }
    fn child_views(&self, context: PaintContext) -> Result<Vec<View>, LayoutError> {
        let window = self.window(context.id)?;
        let mut children = Vec::with_capacity(window.items.len());
        for index in window.items {
            let height = self.properties.sizes.height(index);
            let row = (self.properties.factory)(index);
            let key = row
                .key
                .clone()
                .ok_or(LayoutError::InvalidStyle(context.id))?;
            // The slot owns row geometry; the application's row retains its own
            // style and is clipped to the slot if its natural content is larger.
            children.push(
                crate::column([row])
                    .keyed(key)
                    .clipped(true)
                    .style(|s| {
                        s.width = Dimension::Percent(1.0);
                        if let Some(height) = height {
                            s.height = Dimension::Length(height);
                            s.min_height = Dimension::Length(height);
                            s.max_height = Dimension::Length(height);
                        } else {
                            s.height = Dimension::Auto;
                            s.min_height = Dimension::Length(1.0);
                        }
                        s.shrink = 0.0;
                    })
                    .build()
                    .layout_tagged(LayoutTag {
                        revision: self.measurement_generation,
                        value: index as u64,
                    }),
            );
        }
        Ok(children)
    }
    fn children_prepared(
        &mut self,
        context: PaintContext,
        children: &[ChildLayout],
    ) -> Result<Invalidation, LayoutError> {
        let invalidation = self.record_measurements(context, children)?;
        self.navigation.record(
            children,
            self.properties.navigation_action.is_some(),
            self.measurement_generation,
        );
        Ok(invalidation)
    }
    fn navigate_focus(&mut self, navigation: FocusNavigation) -> FocusNavigationResponse {
        self.navigation.navigate(
            navigation,
            self.properties.navigation_action,
            self.index.as_ref().map_or(0, ExtentIndex::len),
            self.properties.navigable_rows.as_deref(),
        )
    }
    fn enter_focus(&mut self, entry: FocusEntry) -> FocusNavigationResponse {
        navigation::enter(
            entry,
            self.properties.navigation_action,
            self.index.as_ref().map_or(0, ExtentIndex::len),
            self.properties.navigable_rows.as_deref(),
        )
    }
    fn child_viewport(&self, context: PaintContext) -> ChildViewport {
        ChildViewport {
            translation: Point::new(0.0, self.origin - self.offset()),
            clip: Some(context.content_bounds),
        }
    }
    fn enabled(&self) -> bool {
        self.scroll.enabled()
    }
    fn focusable(&self) -> bool {
        self.scroll.focusable()
    }
    fn reveal_child(&mut self, context: PaintContext, area: Rect) -> bool {
        self.scroll.reveal_child(context, area)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        self.scroll.paint(context, painter);
    }
    fn paint_foreground(&self, context: PaintContext, painter: &mut Painter<'_>) {
        self.scroll.paint_foreground(context, painter);
    }
    fn hit_test_foreground(&self, point: Point, context: PaintContext) -> bool {
        self.scroll.hit_test_foreground(point, context)
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        self.scroll.event(context, event)
    }
}
