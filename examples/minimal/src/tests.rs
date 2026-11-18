use super::*;
use std::sync::Arc;
use std::time::Duration;
use zui_api::action::{ActionEvent, ActionId, WidgetAction};
use zui_app::ViewApplication;
use zui_platform::{WindowEvent, WindowMetrics};

#[test]
fn counter_action_reconciles_and_lays_out_without_a_window() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut counter =
        ReactiveApplication::new(|scope, events| Counter::create(scope, events, families));
    let mut tree = zui_core::Tree::from_view(counter.view(), Default::default()).unwrap();
    let event = |id| ActionEvent {
        source: tree.root(),
        action: WidgetAction::new(ActionId(id), ButtonActivated),
    };
    counter.action(&event(99), &tree);
    assert!(!counter.view_is_dirty());
    counter.action(&event(counter.component().increment.id().0), &tree);
    assert!(counter.view_is_dirty());
    assert_eq!(counter.component().count.get(), 1);
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

#[test]
fn host_refreshes_observed_state_after_real_keyboard_activation() {
    use zui_app::Application;
    use zui_platform::{InputEvent, Key, KeyInput};
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let app = ReactiveApplication::new(|scope, events| Counter::create(scope, events, families));
    let mut host = WidgetHost::new(
        app,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap();
    let metrics = WindowMetrics {
        extent: zui_types::Extent::new(360, 180),
        scale: 1.0,
        occluded: false,
    };
    let key = |key, pressed| {
        WindowEvent::Input(InputEvent::Key(KeyInput {
            key,
            pressed,
            repeat: false,
            text: None,
            modifiers: Default::default(),
        }))
    };
    host.update(key(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    assert!(host.tree().focused().is_some());
    host.update(key(Key::Space, true), metrics, Duration::ZERO)
        .unwrap();
    let response = host
        .update(key(Key::Space, false), metrics, Duration::ZERO)
        .unwrap();
    assert!(response.redraw);
    assert_eq!(host.model().component().count.get(), 1);
    assert!(host.model().view_is_dirty());
    // A normal host preparation flushes state; no application rebuild flag.
    host.update(key(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    assert!(!host.model().view_is_dirty());
    assert_eq!(host.model().component().count.get(), 1);
}
