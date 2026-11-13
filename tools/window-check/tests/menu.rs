use std::{sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    event::InputEvent,
    view::View,
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_platform::{WindowAction, WindowEvent, WindowMetrics};
use zui_types::{Extent, Point};
use zui_widgets::{MenuEvent, MenuId, MenuItem, MenuView, PopupPlacement};

struct Model {
    menu: MenuView,
    opened: usize,
}
impl ViewApplication for Model {
    type Message = MenuEvent;
    fn update(&mut self, _: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        ViewAction::default()
    }
    fn view(&self) -> View {
        self.menu
            .popup(PopupPlacement::Point(Point::new(20.0, 20.0)), ActionId(2))
            .unwrap()
    }
    fn map_action(&self, action: &ActionEvent) -> Option<MenuEvent> {
        action.action.payload::<MenuEvent>().cloned()
    }
    fn message(&mut self, message: MenuEvent) -> ViewAction {
        if let MenuEvent::OpenPath(path) = message {
            self.menu.open_path = path;
            self.opened += 1;
        }
        ViewAction {
            rebuild: true,
            window: WindowAction {
                exit: true,
                redraw: true,
                wake_at: Some(Duration::from_secs(2)),
            },
        }
    }
}

#[test]
fn stationary_hover_deadline_reaches_model_without_losing_window_intents() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut branch = MenuItem::new(MenuId(1), "Branch");
    branch.children = vec![MenuItem::new(MenuId(2), "Child")];
    let menu = MenuView::themed(
        &zui_theme::Theme::dashboard(),
        vec![branch],
        families,
        ActionId(1),
    );
    let delay = menu.behavior.hover_delay;
    let mut host = WidgetHost::new(
        Model { menu, opened: 0 },
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap();
    let metrics = WindowMetrics {
        extent: Extent::new(640, 480),
        scale: 1.0,
        occluded: false,
    };
    let mut scene = zui_paint::Scene::new();
    host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
    host.update(
        WindowEvent::Input(InputEvent::PointerMoved(Point::new(40.0, 40.0))),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(host.animation_deadline(), Some(delay));
    host.draw(&mut scene, metrics, delay).unwrap();
    assert_eq!(host.model().opened, 0);
    let action = host.animate_action(metrics, delay).unwrap();
    assert!(action.exit && action.redraw);
    assert_eq!(action.wake_at, Some(Duration::from_secs(2)));
    assert_eq!(host.model().menu.open_path, vec![MenuId(1)]);
    assert_eq!(host.model().opened, 1);
    host.animate_action(metrics, Duration::from_secs(1))
        .unwrap();
    assert_eq!(host.model().opened, 1);
}
