use std::{sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    event::*,
};
use zui_core::Tree;
use zui_theme::Theme;
use zui_types::{Point, Size};
use zui_widgets::{MenuEvent, MenuId, MenuItem, MenuView, PopupPlacement};

struct Fixture {
    tree: Tree,
    text: zui_text::ParleyText,
    menu: MenuView,
    placement: PopupPlacement,
}
impl Fixture {
    fn new(open: bool, left: bool) -> Self {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut first = MenuItem::new(MenuId(1), "First");
        first.children = (10..16)
            .map(|id| MenuItem::new(MenuId(id), "Child"))
            .collect();
        let mut second = MenuItem::new(MenuId(2), "Second");
        second.children = vec![MenuItem::new(MenuId(20), "Other")];
        let mut menu = MenuView::themed(
            &Theme::dashboard(),
            vec![first, second, MenuItem::new(MenuId(3), "Leaf")],
            families,
            ActionId(1),
        );
        menu.width = 180.0;
        if open {
            menu.open_path = vec![MenuId(1)];
        }
        let placement = PopupPlacement::Point(Point::new(if left { 400.0 } else { 20.0 }, 20.0));
        let tree = Tree::from_view(
            menu.popup(placement, ActionId(2)).unwrap(),
            Default::default(),
        )
        .unwrap();
        let mut fixture = Self {
            tree,
            text,
            menu,
            placement,
        };
        fixture.layout();
        fixture
    }
    fn layout(&mut self) {
        self.tree
            .layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut self.text,
                Size::new(640.0, 480.0),
            )
            .unwrap();
    }
    fn reconcile(&mut self) {
        self.tree
            .reconcile(self.menu.popup(self.placement, ActionId(2)).unwrap())
            .unwrap();
        self.layout();
    }
    fn motion(&mut self, horizontal: f64, vertical: f64) {
        assert!(
            self.tree
                .dispatch_input(InputEvent::PointerMoved(Point::new(horizontal, vertical)))
                .unwrap()
                .actions
                .is_empty()
        );
    }
    fn tick(&mut self, milliseconds: u64) -> Vec<ActionEvent> {
        let mut actions = Vec::new();
        self.tree
            .advance_animations_with_actions(Duration::from_millis(milliseconds), &mut actions)
            .unwrap();
        actions
    }
    fn key(&mut self, key: Key) -> Vec<ActionEvent> {
        self.tree
            .dispatch_input(InputEvent::Key(KeyInput {
                key,
                text: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }))
            .unwrap()
            .actions
    }
}
fn expect_path(actions: &[ActionEvent], path: &[u64]) {
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action.payload::<MenuEvent>(),
        Some(&MenuEvent::OpenPath(
            path.iter().map(|id| MenuId(*id)).collect()
        ))
    );
}

#[test]
fn hover_waits_without_new_input_and_does_not_repeat() {
    let mut fixture = Fixture::new(false, false);
    fixture.motion(60.0, 40.0);
    assert!(fixture.tick(0).is_empty());
    assert!(fixture.tick(179).is_empty());
    fixture.reconcile();
    expect_path(&fixture.tick(180), &[1]);
    assert!(fixture.tick(1000).is_empty());
}

#[test]
fn diagonal_travel_preserves_submenu_on_both_sides_and_cancels_on_entry() {
    for left in [false, true] {
        let mut fixture = Fixture::new(true, left);
        let (origin, crossing, child) = if left {
            (550.0, 425.0, 350.0)
        } else {
            (60.0, 170.0, 220.0)
        };
        fixture.motion(origin, 40.0);
        fixture.tick(0);
        fixture.motion(crossing, 85.0);
        assert!(fixture.tick(10).is_empty());
        assert!(fixture.tick(200).is_empty());
        fixture.motion(child, 85.0);
        assert!(fixture.tick(1000).is_empty());
        let position = Point::new(child, 85.0);
        fixture
            .tree
            .dispatch_input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                position: Some(position),
            })
            .unwrap();
        let clicked = fixture
            .tree
            .dispatch_input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: false,
                position: Some(position),
            })
            .unwrap();
        assert!(clicked.actions.iter().any(|action| matches!(
            action.action.payload::<MenuEvent>(),
            Some(MenuEvent::Selected(MenuId(10..=15)))
        )));
    }
}

#[test]
fn resize_cancels_pending_hover_and_rebinding_cannot_emit_old_action() {
    for resize in [false, true] {
        let mut fixture = Fixture::new(false, false);
        fixture.motion(60.0, 40.0);
        fixture.tick(0);
        if resize {
            fixture.menu.width = 200.0;
        } else {
            fixture.menu.action = ActionId(99);
        }
        fixture.reconcile();
        assert!(fixture.tick(1000).is_empty());
    }
}

#[test]
fn third_level_keeps_its_parent_open_during_diagonal_travel() {
    let mut fixture = Fixture::new(true, false);
    fixture.menu.items[0].children[0].children = (30..36)
        .map(|id| MenuItem::new(MenuId(id), "Grandchild"))
        .collect();
    fixture.menu.open_path.push(MenuId(10));
    fixture.reconcile();
    fixture.motion(230.0, 40.0);
    fixture.tick(0);
    fixture.motion(370.0, 85.0);
    assert!(fixture.tick(10).is_empty());
    assert!(fixture.tick(200).is_empty());
    fixture.motion(400.0, 85.0);
    assert!(fixture.tick(1000).is_empty());
    assert_eq!(
        fixture.key(Key::ArrowLeft)[0].action.payload::<MenuEvent>(),
        Some(&MenuEvent::Back)
    );
}

#[test]
fn aim_grace_expires_and_reversing_direction_shortens_it() {
    for reverse in [false, true] {
        let mut fixture = Fixture::new(true, false);
        fixture.motion(60.0, 40.0);
        fixture.tick(0);
        fixture.motion(170.0, 85.0);
        fixture.tick(10);
        if reverse {
            fixture.motion(90.0, 85.0);
            expect_path(&fixture.tick(200), &[2]);
        } else {
            assert!(fixture.tick(359).is_empty());
            expect_path(&fixture.tick(360), &[2]);
        }
    }
}

#[test]
fn keyboard_navigation_ignores_small_pointer_motion_but_allows_deliberate_motion() {
    let mut fixture = Fixture::new(false, false);
    fixture.motion(60.0, 40.0);
    fixture.tick(0);
    fixture.key(Key::End);
    fixture.motion(62.0, 41.0);
    assert!(fixture.tick(1000).is_empty());
    assert_eq!(
        fixture.key(Key::Enter)[0].action.payload::<MenuEvent>(),
        Some(&MenuEvent::Selected(MenuId(3)))
    );
    fixture.motion(90.0, 40.0);
    fixture.tick(1000);
    expect_path(&fixture.tick(1180), &[1]);
}

#[test]
fn leave_scroll_disabled_update_and_escape_cancel_pending_requests() {
    for variant in 0..4 {
        let mut fixture = Fixture::new(false, false);
        fixture.motion(60.0, 40.0);
        fixture.tick(0);
        match variant {
            0 => {
                fixture
                    .tree
                    .dispatch_input(InputEvent::PointerLeft)
                    .unwrap();
            }
            1 => {
                fixture
                    .tree
                    .dispatch_input(InputEvent::Scroll(ScrollDelta::Lines { x: 0.0, y: 1.0 }))
                    .unwrap();
            }
            2 => {
                fixture.menu.items[0].disabled = true;
                fixture.reconcile();
            }
            _ => {
                assert_eq!(fixture.key(Key::Escape).len(), 1);
            }
        }
        assert!(fixture.tick(1000).is_empty());
    }
}

#[test]
fn ordinary_leaf_hover_closes_only_the_child_and_click_is_immediate() {
    let mut fixture = Fixture::new(true, false);
    fixture.motion(50.0, 125.0);
    fixture.tick(0);
    expect_path(&fixture.tick(180), &[]);
    let mut fixture = Fixture::new(false, false);
    fixture.motion(60.0, 40.0);
    for pressed in [true, false] {
        let result = fixture
            .tree
            .dispatch_input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed,
                position: Some(Point::new(60.0, 40.0)),
            })
            .unwrap();
        if !pressed {
            expect_path(&result.actions, &[1]);
        }
    }
    assert!(fixture.tick(1000).is_empty());
}
