use super::*;
use std::sync::Arc;
use zui_api::action::WidgetAction;
#[test]
fn pause_adjust_resume_updates_history_and_all_dashboard_components() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut model = Demo::new(families, None).unwrap();
    let metrics = WindowMetrics {
        extent: zui_types::Extent::new(1100, 800),
        scale: 1.0,
        occluded: false,
    };
    model.update(WindowEvent::Resumed, metrics, Duration::ZERO);
    let mut tree = zui_core::Tree::from_view(model.view(), Default::default()).unwrap();
    for action in [
        WidgetAction::new(ActionId(3), SwitchChanged { checked: false }),
        WidgetAction::new(ActionId(2), SliderChanged { value: 20.0 }),
    ] {
        let message = model
            .map_action(&ActionEvent {
                source: tree.root(),
                action,
            })
            .unwrap();
        assert!(model.message(message).rebuild);
        tree.reconcile(model.view()).unwrap();
        for size in [Size::new(1100.0, 800.0), Size::new(480.0, 640.0)] {
            tree.layout_with_text(&mut zui_layout::TaffyLayout::default(), &mut text, size)
                .unwrap();
            tree.scene().unwrap().validate(Default::default()).unwrap();
        }
    }
    assert!(!model.running);
    assert_eq!(model.next, None);
    assert_eq!(model.value, sample(15) + 20.0);
    assert_eq!(model.history.len(), 17);
    model.update(WindowEvent::Wake, metrics, Duration::from_secs(1));
    assert_eq!(model.history.len(), 17);
    let message = model
        .map_action(&ActionEvent {
            source: tree.root(),
            action: WidgetAction::new(ActionId(1), ButtonActivated),
        })
        .unwrap();
    model.message(message);
    assert!(model.running);
    model.update(WindowEvent::Wake, metrics, Duration::from_secs(1));
    model.update(WindowEvent::Wake, metrics, Duration::from_secs(1) + PERIOD);
    assert_eq!(model.history.len(), 18);
    assert_eq!(model.value, sample(16) + 20.0);
}
