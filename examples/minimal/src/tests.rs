use super::*;
use std::sync::Arc;
use zui_api::action::WidgetAction;

#[test]
fn counter_action_reconciles_and_lays_out_without_a_window() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut counter = Counter { count: 0, families };
    let mut tree = zui_core::Tree::from_view(counter.view(), Default::default()).unwrap();
    let event = |id| ActionEvent {
        source: tree.root(),
        action: WidgetAction::new(ActionId(id), ButtonActivated),
    };
    assert!(counter.map_action(&event(2)).is_none());
    counter.map_action(&event(1)).unwrap();
    assert!(counter.message(()).rebuild);
    assert_eq!(counter.count, 1);
    tree.reconcile(counter.view()).unwrap();
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(360.0, 180.0),
    )
    .unwrap();
    tree.scene().unwrap().validate(Default::default()).unwrap();
    assert!(
        counter
            .update(
                WindowEvent::CloseRequested,
                WindowMetrics {
                    extent: zui_types::Extent::new(360, 180),
                    scale: 1.0,
                    occluded: false,
                },
                Duration::ZERO
            )
            .window
            .exit
    );
}
