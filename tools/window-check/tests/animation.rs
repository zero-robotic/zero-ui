use std::{sync::Arc, time::Duration};
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, LayoutStyle},
    view::View,
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::Scene;
use zui_platform::{WindowEvent, WindowMetrics};
use zui_types::Extent;
use zui_widgets::TextInputView;
struct Model {
    input: TextInputView,
    wakes: usize,
}
impl ViewApplication for Model {
    type Message = ();
    fn view(&self) -> View {
        View::new(self.input.clone()).styled(LayoutStyle {
            width: Dimension::Length(200.0),
            height: Dimension::Length(40.0),
            ..Default::default()
        })
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.wakes += 1;
        }
        ViewAction::default()
    }
}
#[test]
fn host_publishes_caret_deadlines_without_delivering_business_events() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let model = Model {
        input: TextInputView::themed(&zui_theme::Theme::dark(), "中文", families, ActionId(1)),
        wakes: 0,
    };
    let mut host = WidgetHost::new(
        model,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap();
    let metrics = WindowMetrics {
        extent: Extent::new(240, 60),
        scale: 1.0,
        occluded: false,
    };
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.animation_deadline(), None);
    host.update(
        WindowEvent::Input(InputEvent::Key(KeyInput {
            key: Key::Tab,
            pressed: true,
            repeat: false,
            text: None,
            modifiers: Default::default(),
        })),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(host.animation_deadline(), Some(Duration::from_millis(500)));
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    let request = host.ime_request();
    assert!(!host.animate(metrics, Duration::from_millis(499)).unwrap());
    assert!(host.animate(metrics, Duration::from_millis(500)).unwrap());
    host.draw(&mut Scene::new(), metrics, Duration::from_millis(500))
        .unwrap();
    assert_eq!(host.ime_request(), request);
    assert_eq!(host.animation_deadline(), Some(Duration::from_secs(1)));
    assert_eq!(host.model().wakes, 0);
    host.update(
        WindowEvent::Focused(false),
        metrics,
        Duration::from_millis(600),
    )
    .unwrap();
    assert_eq!(host.animation_deadline(), None);
    assert!(
        !host
            .animate(
                WindowMetrics {
                    occluded: true,
                    ..metrics
                },
                Duration::from_secs(1)
            )
            .unwrap()
    );
    assert_eq!(host.animation_deadline(), None);
    host.disconnected();
    assert_eq!(host.animation_deadline(), None);
}
