use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    ScrollArea, Text, TextWrap, Widget, WidgetId,
};
use zui_core::{Rect, Size};
/// Read-only long-form text with a clipped, scrollable viewport.
pub struct TextView {
    id: WidgetId,
    scroll: ScrollArea,
}
impl TextView {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            scroll: ScrollArea::vertical(Text::new(text).wrap(TextWrap::Word)),
        }
    }
    pub fn height(mut self, value: f32) -> Self {
        self.scroll = self.scroll.height(value);
        self
    }
}
impl Widget for TextView {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.scroll.bounds()
    }
    fn measure(&mut self, c: Constraints) -> Size {
        self.scroll.measure(c)
    }
    fn arrange(&mut self, b: Rect) {
        self.scroll.arrange(b)
    }
    fn event(&mut self, e: &UiEvent, c: &mut EventContext) -> EventResult {
        self.scroll.event(e, c)
    }
    fn set_theme(&mut self, t: &Theme) {
        self.scroll.set_theme(t)
    }
    fn build_render_node(&self, t: &Theme) -> zui_render::RenderNode {
        self.scroll.build_render_node(t)
    }
    fn build_render_node_incremental(
        &self,
        context: &mut crate::widget::RenderBuildContext<'_>,
    ) -> zui_render::RenderNode {
        // TextView is a transparent facade over ScrollArea. Delegating only
        // `build_render_node` makes the default incremental path localize the
        // scroll root but leaves its text child in window coordinates, so the
        // parent translation is applied twice and the viewport clips it out.
        self.scroll.build_render_node_incremental(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{widget::RenderBuildContext, WidgetRuntimeTable};
    use zui_core::{Dip, Point};
    use zui_render::{PaintCommand, Transform};

    #[test]
    fn incremental_text_view_localizes_its_scrolled_text_child() {
        let mut view = TextView::new("TextView 内容应位于裁剪区域内").height(120.0);
        view.measure(Constraints::loose(Size {
            width: Dip(320.0),
            height: Dip(400.0),
        }));
        view.arrange(Rect {
            origin: Point {
                x: Dip(380.0),
                y: Dip(240.0),
            },
            size: view.bounds().size,
        });

        let theme = Theme::default();
        let runtime = WidgetRuntimeTable::default();
        let mut context = RenderBuildContext::new(None, &[], true, &theme, &runtime);
        let node = view.build_render_node_incremental(&mut context);

        assert_eq!(node.transform, Transform::translate(Dip(380.0), Dip(240.0)));
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].transform, Transform::IDENTITY);
        assert!(node.children[0]
            .commands
            .iter()
            .any(|command| matches!(command, PaintCommand::Text { .. })));
    }
}
