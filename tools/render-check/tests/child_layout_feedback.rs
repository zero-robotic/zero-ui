use std::{cell::RefCell, rc::Rc, sync::Arc};
use zui_api::{
    layout::{Dimension, LayoutError, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_core::Tree;
use zui_paint::Painter;
use zui_types::Size;
use zui_widgets::TextView;

#[derive(Clone)]
struct MeasuredContent {
    row: TextView,
    sizes: Rc<RefCell<Vec<Size>>>,
}
impl Widget for MeasuredContent {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn child_views(&self, _: PaintContext) -> Result<Vec<View>, LayoutError> {
        Ok(vec![View::new(self.row.clone()).keyed("text")])
    }
    fn children_prepared(
        &mut self,
        _: PaintContext,
        children: &[ChildLayout],
    ) -> Result<Invalidation, LayoutError> {
        if let [child] = children {
            self.sizes.borrow_mut().push(child.layout.bounds.size);
        }
        Ok(Invalidation::None)
    }
}
impl ViewSpec for MeasuredContent {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
#[test]
fn feedback_observes_actual_text_wrapping_after_viewport_width_changes() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let props = MeasuredContent {
        row: TextView::themed(
            &zui_theme::Theme::dark(),
            "布局反馈读取实际文字尺寸，宽度变化后重新换行，不使用估计高度。",
            families,
            zui_theme::TypeRole::Body,
        ),
        sizes: sizes.clone(),
    };
    let view = View::new(props)
        .styled(LayoutStyle {
            kind: LayoutKind::Column,
            width: Dimension::Percent(1.0),
            ..Default::default()
        })
        .managed_children();
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    let mut layout = zui_layout::TaffyLayout::default();
    tree.layout_with_text(&mut layout, &mut text, Size::new(300.0, 400.0))
        .unwrap();
    let child = tree.children(tree.root()).unwrap()[0];
    let wide = *sizes.borrow().last().unwrap();
    assert_eq!(wide, tree.layout_box(child).unwrap().bounds.size);
    assert_eq!(wide.width, 300.0);
    tree.layout_with_text(&mut layout, &mut text, Size::new(120.0, 400.0))
        .unwrap();
    let narrow = *sizes.borrow().last().unwrap();
    assert_eq!(tree.children(tree.root()).unwrap(), &[child]);
    assert_eq!(narrow, tree.layout_box(child).unwrap().bounds.size);
    assert_eq!(narrow.width, 120.0);
    assert!(narrow.height > wide.height);
    assert!(tree.scene().is_ok());
}
