use crate::{
    event::{Action, EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{PaintContext, Widget},
};
use zui_core::Rect;
use zui_render::DisplayList;

pub struct WidgetTree {
    root: Box<dyn Widget>,
    theme: Theme,
    layout_dirty: bool,
    paint_dirty: bool,
    dirty_region: Option<Rect>,
}

impl WidgetTree {
    pub fn new(root: impl Widget + 'static) -> Self {
        Self {
            root: Box::new(root),
            theme: Theme::default(),
            layout_dirty: true,
            paint_dirty: true,
            dirty_region: None,
        }
    }
    pub fn measure(&mut self, constraints: Constraints) -> zui_core::Size {
        self.root.measure(constraints)
    }
    pub fn arrange(&mut self, bounds: zui_core::Rect) {
        self.root.arrange(bounds);
    }
    pub fn layout(&mut self, constraints: Constraints) -> zui_core::Size {
        let size = self.measure(constraints);
        let origin = self.root.bounds().origin;
        self.arrange(zui_core::Rect { origin, size });
        self.layout_dirty = false;
        self.paint_dirty = true;
        self.dirty_region = None;
        size
    }
    pub fn event(&mut self, event: &UiEvent) -> (EventResult, Vec<Action>) {
        let mut ctx = EventContext::new();
        let result = self.root.event(event, &mut ctx);
        if result == EventResult::RequestRedraw {
            self.request_paint(None);
        }
        (result, ctx.take_actions())
    }
    pub fn paint(&self, display_list: &mut DisplayList) {
        self.root
            .paint(&mut PaintContext::new(display_list, &self.theme));
    }
    pub fn theme(&self) -> &Theme {
        &self.theme
    }
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.root.set_theme(&self.theme);
        self.request_paint(None);
    }
    pub fn request_layout(&mut self) {
        self.layout_dirty = true;
        self.paint_dirty = true;
        self.dirty_region = None;
    }
    pub fn request_paint(&mut self, region: Option<Rect>) {
        self.paint_dirty = true;
        if let Some(region) = region {
            self.dirty_region = Some(match self.dirty_region {
                Some(current) => union_rect(current, region),
                None => region,
            });
        } else {
            self.dirty_region = None;
        }
    }
    pub fn needs_redraw(&self) -> bool {
        self.layout_dirty || self.paint_dirty
    }
    pub fn dirty_region(&self) -> Option<Rect> {
        self.dirty_region
    }
    pub fn mark_clean(&mut self) {
        self.layout_dirty = false;
        self.paint_dirty = false;
        self.dirty_region = None;
    }
    pub fn root(&self) -> &dyn Widget {
        &*self.root
    }
    pub fn root_mut(&mut self) -> &mut dyn Widget {
        &mut *self.root
    }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.0.min(b.origin.x.0);
    let top = a.origin.y.0.min(b.origin.y.0);
    let right = (a.origin.x.0 + a.size.width.0).max(b.origin.x.0 + b.size.width.0);
    let bottom = (a.origin.y.0 + a.size.height.0).max(b.origin.y.0 + b.size.height.0);
    Rect {
        origin: zui_core::Point {
            x: zui_core::Dip(left),
            y: zui_core::Dip(top),
        },
        size: zui_core::Size {
            width: zui_core::Dip(right - left),
            height: zui_core::Dip(bottom - top),
        },
    }
}
