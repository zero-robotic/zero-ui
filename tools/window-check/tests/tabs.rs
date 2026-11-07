use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    event::{InputEvent, Key, KeyInput},
    layout::{Dimension, LayoutStyle},
    view::{View, ViewSpec},
    widget::{Invalidation, Widget},
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::Scene;
use zui_platform::{WindowEvent, WindowMetrics};
use zui_theme::Theme;
use zui_types::Extent;
use zui_widgets::{TabChanged, TabId, TabOption, TabsError, TabsView};

type Log = Rc<RefCell<Vec<(char, TabId)>>>;
struct Page(TabId, Log);
impl Widget for Page {
    fn paint(&self, _: zui_api::widget::PaintContext, _: &mut zui_paint::Painter<'_>) {}
    fn focusable(&self) -> bool {
        true
    }
}
impl Drop for Page {
    fn drop(&mut self) {
        self.1.borrow_mut().push(('D', self.0));
    }
}
struct PageView(TabId, Log);
impl ViewSpec for PageView {
    type Widget = Page;
    fn build(&self) -> Page {
        self.1.borrow_mut().push(('B', self.0));
        Page(self.0, self.1.clone())
    }
    fn update(&self, _: &mut Page) -> Invalidation {
        Invalidation::None
    }
}
struct Model {
    families: Vec<String>,
    selected: TabId,
    reversed: bool,
    log: Log,
}
impl ViewApplication for Model {
    type Message = TabId;
    fn view(&self) -> View {
        let mut tabs = vec![
            TabOption::new(TabId(1), "第一页"),
            TabOption::new(TabId(2), "禁用"),
            TabOption::new(TabId(3), "第三页"),
        ];
        tabs[1].disabled = true;
        if self.reversed {
            tabs.reverse();
        }
        let mut view = TabsView::themed(
            &Theme::dark(),
            tabs,
            self.families.clone(),
            Some(self.selected),
            ActionId(42),
        );
        view.layout.width = Dimension::Length(320.0);
        view.build(|id| {
            self.log.borrow_mut().push(('F', id));
            View::new(PageView(id, self.log.clone()))
                .keyed("editor")
                .styled(LayoutStyle {
                    width: Dimension::Length(100.0),
                    height: Dimension::Length(30.0),
                    ..Default::default()
                })
        })
        .unwrap()
    }
    fn map_action(&self, action: &ActionEvent) -> Option<TabId> {
        assert_eq!(action.action.id, ActionId(42));
        action
            .action
            .payload::<TabChanged>()
            .map(|event| event.selected)
    }
    fn message(&mut self, selected: TabId) -> ViewAction {
        self.selected = selected;
        ViewAction::rebuild()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.reversed = !self.reversed;
            if !self.reversed {
                self.selected = TabId(1);
            }
            ViewAction::rebuild()
        } else {
            ViewAction::default()
        }
    }
}
fn input(key: Key) -> WindowEvent {
    WindowEvent::Input(InputEvent::Key(KeyInput {
        key,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    }))
}
#[test]
fn tabs_reconcile_selected_page_and_preserve_header_focus() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let log = Log::default();
    let mut host = WidgetHost::new(
        Model {
            families,
            selected: TabId(1),
            reversed: false,
            log: log.clone(),
        },
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap();
    let metrics = WindowMetrics {
        extent: Extent::new(320, 160),
        scale: 1.0,
        occluded: false,
    };
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    let root = host.tree().root();
    let header = host.tree().children(root).unwrap()[0];
    let panel1 = host.tree().children(root).unwrap()[1];
    let page1 = host.tree().children(panel1).unwrap()[0];
    host.update(input(Key::Tab), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(header));
    host.update(input(Key::ArrowRight), metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().selected, TabId(3));
    assert_eq!(host.tree().focused(), Some(header));
    assert!(!host.tree().contains(page1));
    assert!(!host.tree().contains(panel1));
    let panel3 = host.tree().children(root).unwrap()[1];
    let page3 = host.tree().children(panel3).unwrap()[0];
    host.update(input(Key::Tab), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(page3));
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(page3));
    assert_eq!(host.tree().children(root).unwrap()[1], panel3);
    // External selection removes the focused page even when its child key is identical.
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert!(!host.tree().contains(page3));
    assert!(host.tree().focused().is_none());
    let entries = log.borrow();
    assert!(!entries.iter().any(|(_, id)| *id == TabId(2)));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| **entry == ('B', TabId(3)))
            .count(),
        1
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| **entry == ('D', TabId(3)))
            .count(),
        1
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| **entry == ('B', TabId(1)))
            .count(),
        2
    );
}
#[test]
fn invalid_or_empty_selection_never_builds_content() {
    let make =
        |tabs, selected| TabsView::themed(&Theme::dark(), tabs, Vec::new(), selected, ActionId(1));
    let never = |_| -> View { panic!("factory must not run") };
    assert!(matches!(
        make(
            vec![TabOption::new(TabId(1), "a"), TabOption::new(TabId(1), "b")],
            Some(TabId(1))
        )
        .build(never),
        Err(TabsError::DuplicateId(TabId(1)))
    ));
    assert!(matches!(
        make(Vec::new(), Some(TabId(9))).build(never),
        Err(TabsError::UnknownSelection(TabId(9)))
    ));
    assert!(make(Vec::new(), None).build(never).is_ok());
    assert!(
        make(vec![TabOption::new(TabId(1), "a")], None)
            .build(never)
            .is_ok()
    );
    let mut disabled = TabOption::new(TabId(1), "a");
    disabled.disabled = true;
    assert!(
        make(vec![disabled], Some(TabId(1)))
            .build(|id| {
                assert_eq!(id, TabId(1));
                zui_widgets::Container::empty().build()
            })
            .is_ok()
    );
}
