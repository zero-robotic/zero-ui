use std::{sync::Arc, time::Duration};
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, LayoutStyle},
    view::View,
};
use zui_core::Tree;
use zui_theme::Theme;
use zui_types::{Point, Rect, Size};
use zui_widgets::*;
fn text() -> (zui_text::ParleyText, Vec<String>) {
    let mut t = zui_text::ParleyText::default();
    let f = t
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    (t, f)
}
fn layout(tree: &mut Tree, text: &mut zui_text::ParleyText) {
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        text,
        Size::new(320.0, 240.0),
    )
    .unwrap();
}
fn key(tree: &mut Tree, key: Key) -> zui_core::DispatchResult {
    let text = if let Key::Character(s) = &key {
        Some(s.clone())
    } else {
        None
    };
    tree.dispatch_input(InputEvent::Key(KeyInput {
        key,
        pressed: true,
        repeat: false,
        text,
        modifiers: Default::default(),
    }))
    .unwrap()
}
fn button(f: &[String], label: &str) -> View {
    View::new(ButtonView::themed(
        &Theme::dashboard(),
        label,
        f.to_vec(),
        ActionId(1),
    ))
}
fn root(f: &[String], levels: usize) -> View {
    let mut children = vec![button(f, "base").keyed("base").styled(LayoutStyle {
        width: Dimension::Length(80.0),
        height: Dimension::Length(30.0),
        ..Default::default()
    })];
    for i in 0..levels {
        children.push(
            PopupView::new(PopupPlacement::Center, Size::new(120.0, 80.0), ActionId(9))
                .build(button(f, "popup"))
                .keyed(i as u64),
        );
    }
    stack(children)
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
        })
        .build()
}
#[test]
fn nested_scopes_confine_input_and_restore_focus_without_click_through() {
    let (mut text, f) = text();
    let mut tree = Tree::from_view(root(&f, 0), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let base = tree.children(tree.root()).unwrap()[0];
    tree.request_focus(Some(base)).unwrap();
    tree.reconcile(root(&f, 1)).unwrap();
    layout(&mut tree, &mut text);
    let first = tree.focused().unwrap();
    assert_ne!(first, base);
    assert!(tree.request_focus(Some(base)).is_err());
    key(&mut tree, Key::Tab);
    assert_eq!(tree.focused(), Some(first));
    tree.reconcile(root(&f, 2)).unwrap();
    layout(&mut tree, &mut text);
    assert_ne!(tree.focused(), Some(first));
    assert!(key(&mut tree, Key::Character("x".into())).handled);
    let escape = key(&mut tree, Key::Escape);
    assert!(matches!(
        escape.actions[0].action.payload::<PopupDismissed>(),
        Some(PopupDismissed {
            reason: DismissReason::Escape
        })
    ));
    tree.reconcile(root(&f, 1)).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.focused(), Some(first));
    let click = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            position: Some(Point::new(10.0, 10.0)),
        })
        .unwrap();
    assert_eq!(click.actions.len(), 1);
    assert!(
        click.actions[0]
            .action
            .payload::<PopupDismissed>()
            .is_some()
    );
    tree.reconcile(root(&f, 0)).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.focused(), Some(base));
}
#[test]
fn popup_flips_and_clamps_at_viewport_edges() {
    assert!(
        PopupView::new(PopupPlacement::Center, Size::new(-1.0, 2.0), ActionId(1))
            .bounds(Size::new(320.0, 240.0))
            .is_err()
    );
    let p = PopupView::new(
        PopupPlacement::Anchor(Rect::new(300.0, 220.0, 20.0, 20.0)),
        Size::new(120.0, 80.0),
        ActionId(1),
    );
    assert_eq!(
        p.bounds(Size::new(320.0, 240.0)).unwrap(),
        Rect::new(192.0, 140.0, 120.0, 80.0)
    );
    assert_eq!(
        p.bounds(Size::new(80.0, 40.0)).unwrap().size,
        Size::new(80.0, 40.0)
    );
}
#[test]
fn dropdown_navigation_is_tentative_until_enter_and_skips_disabled() {
    let (mut text, f) = text();
    let mut disabled = ListItem::new(ListItemId(2), "disabled");
    disabled.disabled = true;
    let select = SelectView::themed(
        &Theme::dashboard(),
        vec![
            ListItem::new(ListItemId(1), "one"),
            disabled,
            ListItem::new(ListItemId(3), "three"),
        ],
        Some(ListItemId(1)),
        f,
        ActionId(2),
    );
    let mut tree = Tree::from_view(
        select.popup(Rect::new(0.0, 0.0, 180.0, 32.0), 160.0, ActionId(9)),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    assert!(key(&mut tree, Key::ArrowDown).actions.is_empty());
    let result = key(&mut tree, Key::Enter);
    assert_eq!(
        result.actions[0].action.payload::<SelectEvent>(),
        Some(&SelectEvent::Selected(ListItemId(3)))
    );
}
#[test]
fn menu_submenu_keyboard_and_validation() {
    let (mut text, f) = text();
    let mut item = MenuItem::new(MenuId(1), "submenu");
    item.children = vec![MenuItem::new(MenuId(2), "command")];
    let mut menu = MenuView::themed(
        &Theme::dashboard(),
        vec![MenuItem::new(MenuId(9), "other"), item],
        f,
        ActionId(3),
    );
    let view = menu
        .popup(PopupPlacement::Point(Point::ZERO), ActionId(9))
        .unwrap();
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    layout(&mut tree, &mut text);
    key(&mut tree, Key::End);
    let result = key(&mut tree, Key::ArrowRight);
    assert_eq!(
        result.actions[0].action.payload::<MenuEvent>(),
        Some(&MenuEvent::OpenPath(vec![MenuId(1)]))
    );
    menu.open_path.push(MenuId(1));
    tree.reconcile(
        menu.popup(PopupPlacement::Point(Point::ZERO), ActionId(9))
            .unwrap(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::Enter).actions[0]
            .action
            .payload::<MenuEvent>(),
        Some(&MenuEvent::Selected(MenuId(2)))
    );
    assert_eq!(
        key(&mut tree, Key::ArrowLeft).actions[0]
            .action
            .payload::<MenuEvent>(),
        Some(&MenuEvent::Back)
    );
    menu.open_path.pop();
    tree.reconcile(
        menu.popup(PopupPlacement::Point(Point::ZERO), ActionId(9))
            .unwrap(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::Enter).actions[0]
            .action
            .payload::<MenuEvent>(),
        Some(&MenuEvent::OpenPath(vec![MenuId(1)]))
    );
    menu.open_path.push(MenuId(99));
    assert!(menu.popup(PopupPlacement::Center, ActionId(9)).is_err());
}
#[test]
fn number_editor_rejects_invalid_commits_steps_and_cancels() {
    let (mut text, f) = text();
    let input = NumberInputView::themed(
        &Theme::dashboard(),
        50.0,
        NumericRange::new(0.0, 100.0, 5.0).unwrap(),
        f,
        ActionId(4),
    );
    let mut tree = Tree::from_view(
        View::new(input).styled(LayoutStyle {
            width: Dimension::Length(200.0),
            height: Dimension::Length(40.0),
            ..Default::default()
        }),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    assert_eq!(
        key(&mut tree, Key::ArrowUp).actions[0]
            .action
            .payload::<NumberChanged>(),
        Some(&NumberChanged { value: 55.0 })
    );
    tree.dispatch_input(InputEvent::Key(KeyInput {
        key: Key::Character("a".into()),
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    }))
    .unwrap();
    key(&mut tree, Key::Backspace);
    layout(&mut tree, &mut text);
    key(&mut tree, Key::Character("-".into()));
    layout(&mut tree, &mut text);
    assert!(
        key(&mut tree, Key::Enter).actions[0]
            .action
            .payload::<NumberRejected>()
            .is_some()
    );
    key(&mut tree, Key::Escape);
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::Enter).actions[0]
            .action
            .payload::<NumberChanged>(),
        Some(&NumberChanged { value: 50.0 })
    );
}
#[test]
fn tooltip_delays_hides_and_never_takes_focus_or_pointer() {
    let (mut text, f) = text();
    let families = f.clone();
    let view = TooltipView::themed(
        &Theme::dashboard(),
        f,
        vec![TooltipAnchor {
            bounds: Rect::new(0.0, 0.0, 100.0, 40.0),
            text: "hint".into(),
        }],
        move || {
            button(&families, "target").styled(LayoutStyle {
                width: Dimension::Length(100.0),
                height: Dimension::Length(40.0),
                ..Default::default()
            })
        },
    )
    .build();
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let initial = tree.len();
    let target = tree.hit_test(Point::new(5.0, 5.0)).unwrap().unwrap().id;
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(5.0, 5.0)))
        .unwrap();
    assert_eq!(
        tree.advance_animations(Duration::ZERO).unwrap().next_frame,
        Some(Duration::from_millis(500))
    );
    tree.advance_animations(Duration::from_millis(499)).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.len(), initial);
    tree.advance_animations(Duration::from_millis(500)).unwrap();
    layout(&mut tree, &mut text);
    assert!(tree.len() > initial);
    assert_eq!(tree.focused(), None);
    assert_eq!(
        tree.hit_test(Point::new(5.0, 5.0)).unwrap().unwrap().id,
        target
    );
    tree.request_focus(Some(target)).unwrap();
    key(&mut tree, Key::Space);
    layout(&mut tree, &mut text);
    assert_eq!(
        tree.len(),
        initial,
        "keyboard activation hides background hints"
    );
    tree.dispatch_input(InputEvent::PointerLeft).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.len(), initial);
}

#[test]
fn placeholder_readonly_and_validation_state_survive_updates() {
    let (mut text, f) = text();
    let mut input = TextInputView::themed(&Theme::dashboard(), "", f, ActionId(1));
    input.placeholder = "请输入名称".into();
    input.read_only = true;
    let mut tree = Tree::from_view(View::new(input.clone()), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    let scene = tree.scene().unwrap();
    assert!(
        scene
            .commands()
            .iter()
            .any(|c| matches!(c, zui_paint::Command::Glyphs { .. }))
    );
    assert!(
        key(&mut tree, Key::Character("x".into()))
            .actions
            .is_empty()
    );
    tree.update_widget::<TextInput>(tree.root(), zui_api::widget::Invalidation::None, |w| {
        assert_eq!(w.text(), Some(""))
    })
    .unwrap();
    input.read_only = false;
    input.invalid = true;
    tree.reconcile(View::new(input)).unwrap();
    layout(&mut tree, &mut text);
    assert!(key(&mut tree,Key::Character("x".into())).actions.iter().any(|a|matches!(a.action.payload::<TextInputEvent>(),Some(TextInputEvent::Changed(s)) if s=="x")));
}

fn verify_popup_pixels(renderer: &mut impl zui_paint::OffscreenRenderer) {
    use zui_types::{Color, Extent};
    let mut popup = PopupView::new(PopupPlacement::Center, Size::new(120.0, 80.0), ActionId(1));
    popup.backdrop = Color::rgba(0, 0, 0, 128);
    let content = stack([]).background(Color::rgb(0, 255, 0)).build();
    let mut tree = Tree::from_view(popup.build(content), Default::default()).unwrap();
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(320.0, 240.0),
    )
    .unwrap();
    for scale in [1, 2] {
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                zui_paint::Frame {
                    extent: Extent::new(320 * scale, 240 * scale),
                    scale: scale as f64,
                    background: Color::rgb(0, 0, 255),
                },
            )
            .unwrap();
        for (x, y, color) in [(160, 120, [0, 255, 0, 255]), (10, 10, [0, 0, 127, 255])] {
            let i = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
            assert_eq!(&pixels.pixels()[i..i + 4], &color);
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn popup_pixels_cpu() {
    verify_popup_pixels(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter"]
fn popup_pixels_gpu() {
    verify_popup_pixels(
        &mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap(),
    );
}

#[test]
fn menu_shortcuts_respect_disabled_branches_and_repeat() {
    let mut item = MenuItem::new(MenuId(1), "save");
    item.shortcut = Some(MenuShortcut {
        key: Key::Character("s".into()),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    });
    let menu = MenuView::themed(&Theme::dashboard(), vec![item], Vec::new(), ActionId(1));
    let mut key = KeyInput {
        key: Key::Character("s".into()),
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
    };
    assert_eq!(menu.resolve_shortcut(&key), Some(MenuId(1)));
    key.repeat = true;
    assert_eq!(menu.resolve_shortcut(&key), None);
    let mut menu = menu;
    menu.items[0].disabled = true;
    key.repeat = false;
    assert_eq!(menu.resolve_shortcut(&key), None);
}

#[test]
fn password_masks_graphemes_preserves_navigation_and_blocks_copy_cut() {
    let (mut text, f) = text();
    let secret = "中e\u{301}👩‍💻";
    let mut input = TextInputView::themed(&Theme::dashboard(), secret, f, ActionId(1));
    input.password = true;
    let mut tree = Tree::from_view(View::new(input.clone()), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    let masked = tree.scene().unwrap();
    key(&mut tree, Key::End);
    key(&mut tree, Key::ArrowLeft);
    tree.update_widget::<TextInput>(tree.root(), zui_api::widget::Invalidation::None, |w| {
        assert_eq!(w.text(), Some(secret));
        assert_eq!(w.selection().unwrap().focus, "中e\u{301}".len());
    })
    .unwrap();
    struct NeverCopy;
    impl zui_api::clipboard::Clipboard for NeverCopy {
        fn read_text(&mut self, _: usize) -> Result<String, zui_api::clipboard::ClipboardError> {
            Ok("新".into())
        }
        fn write_text(&mut self, _: &str) -> Result<(), zui_api::clipboard::ClipboardError> {
            panic!("password must not reach clipboard")
        }
    }
    for command in ["a", "c", "x"] {
        tree.dispatch_input_with_services(
            InputEvent::Key(KeyInput {
                key: Key::Character(command.into()),
                pressed: true,
                repeat: false,
                text: None,
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
            }),
            &mut EventServices {
                clipboard: &mut NeverCopy,
            },
        )
        .unwrap();
    }
    tree.update_widget::<TextInput>(tree.root(), zui_api::widget::Invalidation::None, |w| {
        assert_eq!(w.text(), Some(secret))
    })
    .unwrap();
    input.password = false;
    tree.reconcile(View::new(input)).unwrap();
    layout(&mut tree, &mut text);
    let plain = tree.scene().unwrap();
    assert_ne!(
        format!("{:?}", masked.commands()),
        format!("{:?}", plain.commands())
    );
}

#[test]
fn multi_selection_ranges_toggle_and_pointer_modifiers_are_controlled() {
    let (mut text, f) = text();
    let mut disabled = ListItem::new(ListItemId(2), "disabled");
    disabled.disabled = true;
    let mut view = ListBoxView::themed(
        &Theme::dashboard(),
        vec![
            ListItem::new(ListItemId(1), "one"),
            disabled,
            ListItem::new(ListItemId(3), "three"),
        ],
        f,
        None,
        ActionId(7),
    );
    view.multi_selection = Some(std::collections::BTreeSet::from([ListItemId(1)]));
    let description = |v: &ListBoxView| {
        View::new(v.clone()).styled(LayoutStyle {
            width: Dimension::Length(200.0),
            height: Dimension::Length(180.0),
            ..Default::default()
        })
    };
    let mut tree = Tree::from_view(description(&view), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    let send = |tree: &mut Tree, key, pressed, modifiers| {
        tree.dispatch_input(InputEvent::Key(KeyInput {
            key,
            pressed,
            repeat: false,
            text: None,
            modifiers,
        }))
        .unwrap()
    };
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let control = Modifiers {
        control: true,
        ..Default::default()
    };
    let result = send(&mut tree, Key::ArrowDown, true, shift);
    let selected = result.actions[0]
        .action
        .payload::<ListMultiSelectionChanged>()
        .unwrap()
        .selected
        .clone();
    assert_eq!(
        selected,
        std::collections::BTreeSet::from([ListItemId(1), ListItemId(3)])
    );
    view.multi_selection = Some(selected);
    tree.reconcile(description(&view)).unwrap();
    layout(&mut tree, &mut text);
    send(&mut tree, Key::Space, true, control);
    let result = send(&mut tree, Key::Space, false, control);
    assert_eq!(
        result.actions[0]
            .action
            .payload::<ListMultiSelectionChanged>()
            .unwrap()
            .selected,
        std::collections::BTreeSet::from([ListItemId(1)])
    );
    view.multi_selection = Some(std::collections::BTreeSet::from([ListItemId(1)]));
    tree.reconcile(description(&view)).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(None).unwrap();
    tree.dispatch_input(InputEvent::ModifiersChanged(control))
        .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(10.0, 10.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(10.0, 10.0)),
    })
    .unwrap();
    let result = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(10.0, 10.0)),
        })
        .unwrap();
    assert!(
        result.actions[0]
            .action
            .payload::<ListMultiSelectionChanged>()
            .unwrap()
            .selected
            .is_empty()
    );
    let result = send(&mut tree, Key::Character("a".into()), true, control);
    assert_eq!(
        result.actions[0]
            .action
            .payload::<ListMultiSelectionChanged>()
            .unwrap()
            .selected
            .len(),
        2
    );
}

#[test]
fn small_dialog_reveals_initial_action_without_waiting_for_more_input() {
    let (mut text, f) = text();
    let view = MessageBoxView::themed(
        &Theme::dashboard(),
        "确认",
        "这是需要在小视口内滚动的长提示。".repeat(20),
        f,
        ActionId(1),
        ActionId(2),
    )
    .build();
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(320.0, 120.0),
    )
    .unwrap();
    // Footer stays visible even when the body needs to scroll.
    let focused = tree.focused().unwrap();
    let bounds = tree.presented_bounds(focused).unwrap();
    assert!(bounds.origin.y >= 0.0);
    assert!(bounds.origin.y + bounds.size.height <= 120.0);
    assert_eq!(
        key(&mut tree, Key::Enter).actions[0]
            .action
            .payload::<DialogResult>(),
        Some(&DialogResult::Accept)
    );
}

#[test]
fn number_input_preserves_ime_candidate_keys_and_unedited_escape() {
    let (mut text, f) = text();
    let input = NumberInputView::themed(
        &Theme::dashboard(),
        50.0,
        NumericRange::new(0.0, 100.0, 5.0).unwrap(),
        f,
        ActionId(1),
    );
    let mut tree = Tree::from_view(View::new(input), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    assert!(!key(&mut tree, Key::Escape).handled);
    tree.dispatch_input(InputEvent::ImePreedit {
        text: "6".into(),
        cursor: None,
    })
    .unwrap();
    layout(&mut tree, &mut text);
    for k in [Key::ArrowUp, Key::ArrowDown, Key::Enter, Key::Escape] {
        let result = key(&mut tree, k);
        assert!(result.handled);
        assert!(result.actions.is_empty());
    }
    tree.dispatch_input(InputEvent::ImeDisabled).unwrap();
    layout(&mut tree, &mut text);
    assert!(!key(&mut tree, Key::Escape).handled);
}

#[test]
fn changing_dialog_result_cancels_an_armed_button_without_losing_focus() {
    let (mut text, f) = text();
    let description = |result| {
        let mut dialog = MessageBoxView::themed(
            &Theme::dashboard(),
            "确认",
            "内容",
            f.clone(),
            ActionId(1),
            ActionId(2),
        );
        dialog.buttons = vec![("操作".into(), result)];
        dialog.build()
    };
    let mut tree = Tree::from_view(description(DialogResult::Accept), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let focused = tree.focused();
    key(&mut tree, Key::Space);
    tree.reconcile(description(DialogResult::Cancel)).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.focused(), focused);
    let release = InputEvent::Key(KeyInput {
        key: Key::Space,
        pressed: false,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    });
    assert!(
        tree.dispatch_input(release.clone())
            .unwrap()
            .actions
            .is_empty()
    );
    key(&mut tree, Key::Space);
    assert_eq!(
        tree.dispatch_input(release).unwrap().actions[0]
            .action
            .payload::<DialogResult>(),
        Some(&DialogResult::Cancel)
    );
}

#[test]
fn dropdown_and_menu_icons_do_not_require_symbol_font_glyphs() {
    let mut text = zui_text::ParleyText::new_with_policy(
        Default::default(),
        zui_text::MissingGlyphPolicy::Reject,
    );
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let select = SelectView::themed(
        &Theme::dashboard(),
        vec![ListItem::new(ListItemId(1), "预设 50%")],
        Some(ListItemId(1)),
        families.clone(),
        ActionId(1),
    );
    let mut item = MenuItem::new(MenuId(1), "设置");
    item.checked = true;
    item.children.push(MenuItem::new(MenuId(2), "确认"));
    let menu = MenuView::themed(&Theme::dashboard(), vec![item], families, ActionId(2));
    for view in [
        select.build(),
        menu.popup(PopupPlacement::Center, ActionId(3)).unwrap(),
    ] {
        let mut tree = Tree::from_view(view, Default::default()).unwrap();
        layout(&mut tree, &mut text);
        let scene = tree.scene().unwrap();
        assert!(
            scene
                .commands()
                .iter()
                .any(|command| matches!(command, zui_paint::Command::Stroke { .. }))
        );
    }
}

#[test]
fn dialog_title_and_close_button_are_independent_and_dismiss_without_accepting() {
    fn find(tree: &Tree, id: zui_types::NodeId, key: &str) -> Option<zui_types::NodeId> {
        if tree.key(id).unwrap() == Some(&zui_api::view::Key::from(key)) {
            return Some(id);
        }
        tree.children(id)
            .unwrap()
            .iter()
            .find_map(|&child| find(tree, child, key))
    }
    let (mut text, f) = text();
    for show_title in [false, true] {
        for show_close_button in [false, true] {
            let mut dialog =
                DialogView::themed(&Theme::desktop_dark(), "确认设置", f.clone(), ActionId(90));
            dialog.show_title = show_title;
            dialog.show_close_button = show_close_button;
            dialog.size = Size::new(300.0, 220.0);
            let body = column((0..30).map(|_| button(&f, "长内容"))).build();
            let mut tree =
                Tree::from_view(dialog.build(body, button(&f, "确认")), Default::default())
                    .unwrap();
            layout(&mut tree, &mut text);
            assert_eq!(
                find(&tree, tree.root(), "dialog-title").is_some(),
                show_title
            );
            let close = find(&tree, tree.root(), "dialog-close");
            assert_eq!(close.is_some(), show_close_button);
            let footer = find(&tree, tree.root(), "dialog-footer").unwrap();
            let bounds = tree.presented_bounds(footer).unwrap();
            assert!(bounds.origin.y + bounds.size.height <= 230.0);
            tree.scene().unwrap().validate(Default::default()).unwrap();
            if let Some(close) = close {
                let bounds = tree.presented_bounds(close).unwrap();
                let point = Point::new(
                    bounds.origin.x + bounds.size.width / 2.0,
                    bounds.origin.y + bounds.size.height / 2.0,
                );
                for pressed in [true, false] {
                    let result = tree
                        .dispatch_input(InputEvent::PointerButton {
                            button: PointerButton::Primary,
                            pressed,
                            position: Some(point),
                        })
                        .unwrap();
                    if !pressed {
                        assert_eq!(result.actions.len(), 1);
                        assert_eq!(result.actions[0].action.id, ActionId(90));
                        assert_eq!(
                            result.actions[0].action.payload::<PopupDismissed>(),
                            Some(&PopupDismissed {
                                reason: DismissReason::CloseButton
                            })
                        );
                        assert!(result.actions[0].action.payload::<DialogResult>().is_none());
                    }
                }
                tree.request_focus(Some(close)).unwrap();
                let result = key(&mut tree, Key::Enter);
                assert_eq!(
                    result.actions[0].action.payload::<PopupDismissed>(),
                    Some(&PopupDismissed {
                        reason: DismissReason::CloseButton
                    })
                );
            }
            let result = key(&mut tree, Key::Escape);
            assert_eq!(
                result.actions[0].action.payload::<PopupDismissed>(),
                Some(&PopupDismissed {
                    reason: DismissReason::Escape
                })
            );
        }
    }
}
