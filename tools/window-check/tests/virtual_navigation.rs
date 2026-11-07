use std::{cell::RefCell, rc::Rc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    event::{InputEvent, Key as InputKey, KeyInput, Modifiers},
    layout::{Dimension, LayoutStyle},
    view::{Key, View, ViewSpec},
    widget::{Invalidation, PaintContext, Widget},
};
use zui_app::{Application, ViewAction, ViewApplication, ViewFocusRequest, WidgetHost};
use zui_core::Tree;
use zui_paint::{Painter, Scene};
use zui_platform::{WindowEvent, WindowMetrics};
use zui_types::{Extent, NodeId};
use zui_widgets::{ScrollAlignment, VirtualListNavigate, VirtualListView, column, row};

#[derive(Clone, Copy)]
struct Control;
impl ViewSpec for Control {
    type Widget = Self;
    fn build(&self) -> Self {
        *self
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
impl Widget for Control {
    fn focusable(&self) -> bool {
        true
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
fn control(key: &str) -> View {
    View::new(Control).keyed(key).styled(LayoutStyle {
        width: Dimension::Length(30.0),
        height: Dimension::Length(20.0),
        ..Default::default()
    })
}
struct Model {
    count: usize,
    index: usize,
    revision: u64,
    pending: Option<bool>,
    messages: usize,
    enabled: bool,
    mode: u8,
    eligible: Option<Rc<[usize]>>,
    built: Rc<RefCell<Vec<usize>>>,
}
impl ViewApplication for Model {
    type Message = VirtualListNavigate;
    fn view(&self) -> View {
        let eligible = self.eligible.clone();
        let built = self.built.clone();
        let factory = move |index| {
            built.borrow_mut().push(index);
            if eligible
                .as_ref()
                .is_some_and(|rows| rows.binary_search(&index).is_err())
            {
                return column([])
                    .keyed(index as u64)
                    .style(|s| s.height = Dimension::Length(20.0))
                    .build();
            }
            row([control("a"), control("b")])
                .keyed(index as u64)
                .build()
        };
        let mut list = match self.mode {
            1 => VirtualListView::variable(vec![20.0; self.count].into(), factory),
            2 => VirtualListView::measured(self.count, 20.0, 0, factory),
            _ => VirtualListView::new(self.count, 20.0, factory),
        }
        .overscan(0)
        .scrollbar(None)
        .keyed("list")
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(20.0);
            s.shrink = 0.0;
        });
        if self.count > 0 {
            list = list.scroll_to_index(self.index, ScrollAlignment::Start, self.revision);
        }
        if let Some(rows) = &self.eligible {
            list = list.navigable_rows(rows.clone());
        }
        if self.enabled {
            list = list.on_navigate(ActionId(7));
        }
        column([control("before"), list.build(), control("after")]).build()
    }
    fn map_action(&self, action: &ActionEvent) -> Option<Self::Message> {
        (action.action.id == ActionId(7))
            .then(|| action.action.payload::<VirtualListNavigate>().copied())
            .flatten()
    }
    fn message(&mut self, message: Self::Message) -> ViewAction {
        assert!(message.index < self.count);
        self.index = message.index;
        self.revision += 1;
        self.pending = Some(message.reverse);
        self.messages += 1;
        ViewAction::rebuild()
    }
    fn take_focus_request(&mut self, tree: &Tree) -> Option<ViewFocusRequest> {
        let reverse = self.pending?;
        let list = child(tree, tree.root(), Key::from("list"))?;
        let slot = child(tree, list, Key::Number(self.index as u64))?;
        let row = tree.children(slot).ok()?[0];
        let target = child(tree, row, Key::from(if reverse { "b" } else { "a" }))?;
        self.pending = None;
        Some(ViewFocusRequest::Focus(target))
    }
    fn update(&mut self, _: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        ViewAction::default()
    }
}
fn child(tree: &Tree, parent: NodeId, key: Key) -> Option<NodeId> {
    tree.children(parent)
        .ok()?
        .iter()
        .copied()
        .find(|id| tree.key(*id).ok().flatten() == Some(&key))
}
type Host = WidgetHost<Model, zui_layout::TaffyLayout, zui_text::ParleyText>;
fn host(count: usize, enabled: bool) -> Host {
    host_mode(count, enabled, 0)
}
fn host_mode(count: usize, enabled: bool, mode: u8) -> Host {
    host_rows(count, enabled, mode, None)
}
fn host_rows(count: usize, enabled: bool, mode: u8, eligible: Option<Rc<[usize]>>) -> Host {
    WidgetHost::new(
        Model {
            count,
            index: 0,
            revision: 0,
            pending: None,
            messages: 0,
            enabled,
            mode,
            eligible,
            built: Default::default(),
        },
        zui_layout::TaffyLayout::default(),
        zui_text::ParleyText::default(),
        Default::default(),
    )
    .unwrap()
}
fn metrics() -> WindowMetrics {
    WindowMetrics {
        extent: Extent::new(100, 100),
        scale: 1.0,
        occluded: false,
    }
}
fn tab(host: &mut Host, reverse: bool) {
    host.update(
        WindowEvent::Input(InputEvent::Key(KeyInput {
            key: InputKey::Tab,
            pressed: true,
            repeat: false,
            text: None,
            modifiers: Modifiers {
                shift: reverse,
                ..Default::default()
            },
        })),
        metrics(),
        Duration::ZERO,
    )
    .unwrap();
    host.draw(&mut Scene::new(), metrics(), Duration::ZERO)
        .unwrap();
}
fn key(host: &Host) -> Key {
    host.tree()
        .key(host.tree().focused().unwrap())
        .unwrap()
        .unwrap()
        .clone()
}
#[test]
fn tab_realizes_missing_rows_in_both_directions_preserving_in_row_order_and_exits() {
    let mut host = host(4, true);
    tab(&mut host, false);
    assert_eq!(key(&host), Key::from("before"));
    tab(&mut host, false);
    assert_eq!(key(&host), Key::from("list"));
    for index in 0..4 {
        tab(&mut host, false);
        assert_eq!(host.model().index, index);
        assert_eq!(key(&host), Key::from("a"));
        let count = host.model().messages;
        tab(&mut host, false);
        assert_eq!(key(&host), Key::from("b"));
        assert_eq!(host.model().messages, count);
        let list = child(host.tree(), host.tree().root(), Key::from("list")).unwrap();
        assert_eq!(host.tree().children(list).unwrap().len(), 1);
    }
    tab(&mut host, false);
    assert_eq!(key(&host), Key::from("after"));
    for index in (0..4).rev() {
        tab(&mut host, true);
        assert_eq!(host.model().index, index);
        assert_eq!(key(&host), Key::from("b"));
        tab(&mut host, true);
        assert_eq!(key(&host), Key::from("a"));
    }
    tab(&mut host, true);
    assert_eq!(key(&host), Key::from("list"));
    tab(&mut host, true);
    assert_eq!(key(&host), Key::from("before"));
    assert_eq!(host.model().messages, 8);
}
#[test]
fn empty_and_opt_out_lists_keep_default_traversal_without_actions() {
    let mut empty = host(0, true);
    for expected in ["before", "list", "after"] {
        tab(&mut empty, false);
        assert_eq!(key(&empty), Key::from(expected));
    }
    assert_eq!(empty.model().messages, 0);
    let mut plain = host(4, false);
    for expected in ["before", "list", "a", "b", "after"] {
        tab(&mut plain, false);
        assert_eq!(key(&plain), Key::from(expected));
    }
    assert_eq!(plain.model().messages, 0);
}

#[test]
fn supplied_and_measured_rows_use_committed_slot_indices_for_navigation() {
    for mode in [1, 2] {
        let mut host = host_mode(4, true, mode);
        for _ in 0..7 {
            tab(&mut host, false);
        }
        assert_eq!(host.model().index, 2);
        assert_eq!(key(&host), Key::from("a"));
        tab(&mut host, true);
        assert_eq!(host.model().index, 1);
        assert_eq!(key(&host), Key::from("b"));
    }
}

#[test]
fn reverse_entry_from_outside_realizes_logical_last_row_in_every_size_mode() {
    for mode in [0, 1, 2] {
        let mut host = host_mode(1000, true, mode);
        tab(&mut host, true);
        assert_eq!(key(&host), Key::from("after"));
        let list = child(host.tree(), host.tree().root(), Key::from("list")).unwrap();
        assert!(child(host.tree(), list, Key::Number(999)).is_none());
        tab(&mut host, true);
        assert_eq!(host.model().index, 999);
        assert_eq!(key(&host), Key::from("b"));
        assert_eq!(host.model().messages, 1);
        assert_eq!(host.tree().children(list).unwrap().len(), 1);
        tab(&mut host, true);
        assert_eq!(key(&host), Key::from("a"));
        assert_eq!(host.model().messages, 1);
    }
}

#[test]
fn declared_navigable_rows_skip_unfocusable_ranges_without_realizing_them() {
    for mode in [0, 1, 2] {
        let mut host = host_rows(1000, true, mode, Some(vec![100, 900].into()));
        tab(&mut host, false); // before
        tab(&mut host, false); // list
        tab(&mut host, false);
        assert_eq!(host.model().index, 100);
        assert_eq!(key(&host), Key::from("a"));
        tab(&mut host, false); // b
        tab(&mut host, false);
        assert_eq!(host.model().index, 900);
        assert_eq!(key(&host), Key::from("a"));
        tab(&mut host, true);
        assert_eq!(host.model().index, 100);
        assert_eq!(key(&host), Key::from("b"));
        assert!(
            host.model()
                .built
                .borrow()
                .iter()
                .all(|index| [0, 100, 900].contains(index))
        );
        let mut reverse = host_rows(1000, true, mode, Some(vec![100, 900].into()));
        tab(&mut reverse, true); // after
        tab(&mut reverse, true);
        assert_eq!(reverse.model().index, 900);
        assert_eq!(key(&reverse), Key::from("b"));
    }
    let mut empty = host_rows(1000, true, 0, Some(Vec::new().into()));
    for expected in ["before", "list", "after"] {
        tab(&mut empty, false);
        assert_eq!(key(&empty), Key::from(expected));
    }
    assert_eq!(empty.model().messages, 0);
}

#[test]
fn navigation_metadata_validation_retries_and_rechecks_shared_rows_after_count_change() {
    use zui_types::Size;
    let view = |count, rows: Rc<[usize]>| {
        VirtualListView::new(count, 20.0, |index| column([]).keyed(index as u64).build())
            .navigable_rows(rows)
            .style(|s| {
                s.width = Dimension::Length(100.0);
                s.height = Dimension::Length(20.0);
            })
            .build()
    };
    for rows in [vec![1, 1], vec![2, 1], vec![4]] {
        let mut tree = Tree::from_view(view(4, rows.into()), Default::default()).unwrap();
        let mut layout = zui_layout::TaffyLayout::default();
        assert!(tree.layout(&mut layout, Size::new(100.0, 20.0)).is_err());
        let good: Rc<[usize]> = vec![1, 3].into();
        tree.reconcile(view(4, good.clone())).unwrap();
        tree.layout(&mut layout, Size::new(100.0, 20.0)).unwrap();
        tree.reconcile(view(3, good)).unwrap();
        assert!(tree.layout(&mut layout, Size::new(100.0, 20.0)).is_err());
        tree.reconcile(view(3, vec![1].into())).unwrap();
        tree.layout(&mut layout, Size::new(100.0, 20.0)).unwrap();
    }
}
