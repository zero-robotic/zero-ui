use std::{cell::RefCell, rc::Rc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId, WidgetAction},
    event::*,
    layout::{Dimension, LayoutStyle},
    view::{View, ViewSpec},
    widget::{Invalidation, PaintContext, Widget},
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::{Painter, Scene};
use zui_platform::{WindowAction, WindowEvent, WindowMetrics};
use zui_types::{Color, Extent, NodeId, Point, Rect};

type Log = Rc<RefCell<Vec<&'static str>>>;
struct Activated;
#[derive(Clone)]
struct Control {
    log: Log,
    pressed: bool,
    value: usize,
}
impl Widget for Control {
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        painter.fill_rect(
            Rect::new(0.0, 0.0, context.size.width, context.size.height),
            if self.value == 0 {
                Color::BLACK
            } else {
                Color::WHITE
            },
        );
    }
    fn focusable(&self) -> bool {
        true
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        if matches!(
            event,
            WidgetEvent::PointerCaptureLost | WidgetEvent::FocusChanged(false)
        ) {
            self.log.borrow_mut().push("cancel");
            self.pressed = false;
        }
        if context.phase != EventPhase::Target {
            return EventResponse::default();
        }
        let mut response = EventResponse::default();
        match event {
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed,
                ..
            }) => {
                response.handled = true;
                response.invalidation = Invalidation::Paint;
                if *pressed {
                    self.pressed = true;
                    response.focus = FocusRequest::Focus;
                    response.capture = CaptureRequest::Capture;
                } else if self.pressed {
                    self.log.borrow_mut().push("route");
                    self.pressed = false;
                    response
                        .actions
                        .push(WidgetAction::new(ActionId(7), Activated));
                }
            }
            WidgetEvent::Input(InputEvent::Key(key)) if key.key == Key::Space && key.pressed => {
                self.log.borrow_mut().push("route");
                response.handled = true;
                response
                    .actions
                    .push(WidgetAction::new(ActionId(7), Activated));
                // Unmapped custom actions are explicitly ignored by this model.
                response
                    .actions
                    .push(WidgetAction::new(ActionId(99), "unknown"));
            }
            WidgetEvent::Input(InputEvent::Key(key)) if key.key == Key::Enter && key.pressed => {
                response.handled = true;
                response
                    .actions
                    .push(WidgetAction::new(ActionId(7), Activated));
                response
                    .actions
                    .push(WidgetAction::new(ActionId(7), Activated));
            }
            _ => {}
        }
        response
    }
}
impl ViewSpec for Control {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, control: &mut Self) -> Invalidation {
        if self.value == control.value {
            Invalidation::None
        } else {
            control.value = self.value;
            Invalidation::Paint
        }
    }
}
enum Message {
    Increment,
}
struct Model {
    log: Log,
    count: usize,
    fallback: usize,
    width: f32,
    source: Rc<RefCell<Option<NodeId>>>,
}
impl ViewApplication for Model {
    type Message = Message;
    fn view(&self) -> View {
        View::new(Control {
            log: self.log.clone(),
            pressed: false,
            value: self.count,
        })
        .keyed("control")
        .styled(LayoutStyle {
            width: Dimension::Length(self.width),
            height: Dimension::Length(40.0),
            ..Default::default()
        })
    }
    fn map_action(&self, event: &ActionEvent) -> Option<Message> {
        if event.action.id == ActionId(7) && event.action.payload::<Activated>().is_some() {
            self.log.borrow_mut().push("map");
            *self.source.borrow_mut() = Some(event.source);
            Some(Message::Increment)
        } else {
            None
        }
    }
    fn message(&mut self, _: Message) -> ViewAction {
        self.log.borrow_mut().push("message");
        self.count += 1;
        ViewAction {
            rebuild: true,
            window: WindowAction {
                wake_at: Some(Duration::from_millis(100 / self.count as u64)),
                ..Default::default()
            },
        }
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        match event {
            WindowEvent::Input(_) => {
                self.fallback += 1;
                ViewAction::default()
            }
            WindowEvent::Wake => {
                self.width = 50.0;
                ViewAction::rebuild()
            }
            _ => ViewAction::default(),
        }
    }
}
fn host() -> WidgetHost<Model, zui_layout::TaffyLayout, zui_text::ParleyText> {
    WidgetHost::new(
        Model {
            log: Log::default(),
            count: 0,
            fallback: 0,
            width: 200.0,
            source: Default::default(),
        },
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap()
}
fn metrics() -> WindowMetrics {
    WindowMetrics {
        extent: Extent::new(300, 200),
        scale: 1.0,
        occluded: false,
    }
}
fn pointer(pressed: bool, x: f64) -> WindowEvent {
    WindowEvent::Input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(x, 20.0)),
    })
}
fn key(key: Key) -> WindowEvent {
    WindowEvent::Input(InputEvent::Key(KeyInput {
        key,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    }))
}
#[test]
fn controls_dispatch_then_map_typed_messages_without_duplicate_window_shortcuts() {
    let mut host = host();
    let root = host.tree().root();
    // Dispatch is valid before the first draw: Host prepares geometry first.
    host.update(pointer(true, 20.0), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(root));
    assert_eq!(host.tree().pointer_capture(), Some(root));
    let action = host
        .update(pointer(false, 20.0), metrics(), Duration::ZERO)
        .unwrap();
    assert!(action.redraw);
    assert_eq!(action.wake_at, Some(Duration::from_millis(100)));
    assert_eq!(host.model().count, 1);
    assert_eq!(host.model().fallback, 0);
    assert_eq!(*host.model().source.borrow(), Some(root));
    let log = host.model().log.borrow().clone();
    let route = log.iter().position(|s| *s == "route").unwrap();
    let map = log.iter().position(|s| *s == "map").unwrap();
    let message = log.iter().position(|s| *s == "message").unwrap();
    assert!(route < map && map < message);
    let mut scene = Scene::new();
    host.draw(&mut scene, metrics(), Duration::ZERO).unwrap();
    assert_eq!(host.tree().root(), root);
    host.update(key(Key::Space), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().count, 2);
    assert_eq!(host.model().fallback, 0);
    host.update(key(Key::Escape), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().fallback, 1);
}
#[test]
fn pending_view_changes_are_applied_before_the_next_hit_test() {
    let mut host = host();
    host.draw(&mut Scene::new(), metrics(), Duration::ZERO)
        .unwrap();
    host.update(WindowEvent::Wake, metrics(), Duration::ZERO)
        .unwrap();
    host.update(pointer(true, 100.0), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().pointer_capture(), None);
    assert_eq!(host.model().fallback, 1);
    assert_eq!(
        host.tree()
            .layout_box(host.tree().root())
            .unwrap()
            .bounds
            .size
            .width,
        50.0
    );
}
#[test]
fn focus_loss_hidden_input_and_disconnect_cancel_ownership_without_activation() {
    let mut host = host();
    host.update(pointer(true, 20.0), metrics(), Duration::ZERO)
        .unwrap();
    host.update(WindowEvent::Focused(false), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), None);
    assert_eq!(host.tree().pointer_capture(), None);
    host.update(pointer(false, 20.0), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().count, 0);
    host.update(pointer(true, 20.0), metrics(), Duration::ZERO)
        .unwrap();
    let mut hidden = metrics();
    hidden.occluded = true;
    host.update(key(Key::Space), hidden, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().count, 0);
    assert_eq!(host.tree().focused(), None);
    host.update(pointer(true, 20.0), metrics(), Duration::ZERO)
        .unwrap();
    host.disconnected();
    assert_eq!(host.tree().pointer_capture(), None);
    assert_eq!(host.tree().focused(), None);
}

#[test]
fn multiple_actions_apply_in_order_and_merge_the_earliest_wake() {
    let mut host = host();
    host.update(key(Key::Tab), metrics(), Duration::ZERO)
        .unwrap();
    let action = host
        .update(key(Key::Enter), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().count, 2);
    assert_eq!(host.model().fallback, 0);
    assert_eq!(action.wake_at, Some(Duration::from_millis(50)));
    assert!(action.redraw);
    assert_eq!(
        host.model().log.borrow().as_slice(),
        &["map", "message", "map", "message"]
    );
}
