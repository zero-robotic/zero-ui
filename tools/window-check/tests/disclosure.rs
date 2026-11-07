use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    event::{InputEvent, Key, KeyInput},
    layout::Dimension,
    view::View,
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::Scene;
use zui_platform::{WindowEvent, WindowMetrics};
use zui_theme::Theme;
use zui_types::Extent;
use zui_widgets::{ButtonView, DisclosureChanged, DisclosureView};
struct Model {
    families: Vec<String>,
    expanded: bool,
    accept: bool,
    disabled: bool,
    requests: usize,
    builds: Rc<Cell<usize>>,
}
impl ViewApplication for Model {
    type Message = bool;
    fn view(&self) -> View {
        let theme = Theme::dark();
        let mut view = DisclosureView::themed(
            &theme,
            "详细设置",
            self.families.clone(),
            self.expanded,
            ActionId(7),
        );
        view.disabled = self.disabled;
        view.layout.width = Dimension::Length(300.0);
        view.build(|| {
            self.builds.set(self.builds.get() + 1);
            View::new(ButtonView::themed(
                &theme,
                "内容操作",
                self.families.clone(),
                ActionId(8),
            ))
            .keyed("child")
        })
    }
    fn map_action(&self, event: &ActionEvent) -> Option<bool> {
        assert_eq!(event.action.id, ActionId(7));
        event
            .action
            .payload::<DisclosureChanged>()
            .map(|event| event.expanded)
    }
    fn message(&mut self, expanded: bool) -> ViewAction {
        self.requests += 1;
        if self.accept {
            self.expanded = expanded;
        }
        ViewAction::rebuild()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.expanded = !self.expanded;
            ViewAction::rebuild()
        } else {
            ViewAction::default()
        }
    }
}
fn input(key: Key, pressed: bool) -> WindowEvent {
    WindowEvent::Input(InputEvent::Key(KeyInput {
        key,
        pressed,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    }))
}
fn host(
    accept: bool,
    disabled: bool,
) -> WidgetHost<Model, zui_layout::TaffyLayout, zui_text::ParleyText> {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    WidgetHost::new(
        Model {
            families,
            expanded: false,
            accept,
            disabled,
            requests: 0,
            builds: Rc::new(Cell::new(0)),
        },
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap()
}
fn metrics() -> WindowMetrics {
    WindowMetrics {
        extent: Extent::new(320, 200),
        scale: 1.0,
        occluded: false,
    }
}
#[test]
fn confirmed_expansion_preserves_header_and_collapse_unmounts_focused_content() {
    let mut host = host(true, false);
    let metrics = metrics();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().builds.get(), 0);
    let root = host.tree().root();
    let header = host.tree().children(root).unwrap()[0];
    host.update(input(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    host.update(input(Key::Enter, true), metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert!(host.model().expanded);
    assert_eq!(host.tree().focused(), Some(header));
    assert_eq!(host.model().builds.get(), 1);
    let panel = host.tree().children(root).unwrap()[1];
    let child = host.tree().children(panel).unwrap()[0];
    host.update(input(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(child));
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert!(!host.tree().contains(panel));
    assert!(!host.tree().contains(child));
    assert!(host.tree().focused().is_none());
    assert_eq!(host.model().builds.get(), 1);
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().children(root).unwrap()[0], header);
    assert_ne!(host.tree().children(root).unwrap()[1], panel);
    assert_eq!(host.model().builds.get(), 2);
}
#[test]
fn requests_are_controlled_and_external_change_cancels_pending_space() {
    let mut host = host(false, false);
    let metrics = metrics();
    host.update(input(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    host.update(input(Key::Enter, true), metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().requests, 1);
    assert!(!host.model().expanded);
    assert_eq!(host.model().builds.get(), 0);
    host.update(input(Key::Space, true), metrics, Duration::ZERO)
        .unwrap();
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    host.update(input(Key::Space, false), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().requests, 1);
    assert!(host.model().expanded);
    host.update(input(Key::Space, true), metrics, Duration::ZERO)
        .unwrap();
    host.update(input(Key::Space, false), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().requests, 2);
    assert!(host.model().expanded);
}
#[test]
fn disabled_header_does_not_disable_expanded_children() {
    let mut host = host(true, true);
    let metrics = metrics();
    host.update(input(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    assert!(host.tree().focused().is_none());
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    let root = host.tree().root();
    let panel = host.tree().children(root).unwrap()[1];
    let child = host.tree().children(panel).unwrap()[0];
    host.update(input(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(child));
    assert_eq!(host.model().requests, 0);
}
