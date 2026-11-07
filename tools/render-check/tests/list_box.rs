use std::sync::Arc;
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{ListBox, ListBoxView, ListItem, ListItemId as Id, ListSelectionChanged};
struct Fixture {
    tree: Tree,
    view: ListBoxView,
    text: zui_text::ParleyText,
}
fn description(view: &ListBoxView) -> View {
    View::new(view.clone()).styled(LayoutStyle {
        width: Dimension::Length(200.0),
        height: Dimension::Length(80.0),
        ..Default::default()
    })
}
impl Fixture {
    fn new() -> Self {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut items: Vec<_> = (1..=10)
            .map(|id| ListItem::new(Id(id), format!("项目 {id}")))
            .collect();
        items[1].disabled = true;
        let mut view = ListBoxView::themed(
            &Theme::dashboard(),
            items,
            families,
            Some(Id(1)),
            ActionId(8),
        );
        view.style.min_row_height = 40.0;
        view.style.padding_y = 0.0;
        let tree = Tree::from_view(description(&view), Default::default()).unwrap();
        let mut f = Self { tree, view, text };
        f.layout();
        f
    }
    fn layout(&mut self) {
        self.tree
            .layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut self.text,
                Size::new(200.0, 80.0),
            )
            .unwrap();
    }
    fn update(&mut self) {
        self.tree.reconcile(description(&self.view)).unwrap();
        self.layout();
    }
    fn key(&mut self, key: Key) -> zui_core::DispatchResult {
        self.tree
            .dispatch_input(InputEvent::Key(KeyInput {
                key,
                pressed: true,
                repeat: false,
                text: None,
                modifiers: Default::default(),
            }))
            .unwrap()
    }
    fn offset(&mut self) -> f64 {
        let mut offset = 0.0;
        self.tree
            .update_widget::<ListBox>(self.tree.root(), Invalidation::None, |w| {
                offset = w.scroll_offset()
            })
            .unwrap();
        offset
    }
    fn click(&mut self, y: f64) -> zui_core::DispatchResult {
        self.tree
            .dispatch_input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                position: Some(Point::new(10.0, y)),
            })
            .unwrap();
        self.tree
            .dispatch_input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: false,
                position: Some(Point::new(10.0, y)),
            })
            .unwrap()
    }
}
#[test]
fn selection_is_controlled_and_keyboard_and_external_updates_reveal_active_rows() {
    let mut f = Fixture::new();
    let root = f.tree.root();
    assert!(f.click(60.0).actions.is_empty()); // Disabled second row.
    f.tree.request_focus(Some(root)).unwrap();
    let result = f.key(Key::ArrowDown);
    assert_eq!(
        result.actions[0].action.payload::<ListSelectionChanged>(),
        Some(&ListSelectionChanged { selected: Id(3) })
    );
    assert_eq!(f.view.selected, Some(Id(1)));
    assert_eq!(f.offset(), 44.0);
    let result = f.key(Key::End);
    assert_eq!(result.actions[0].action.id, ActionId(8));
    assert_eq!(
        result.actions[0].action.payload::<ListSelectionChanged>(),
        Some(&ListSelectionChanged { selected: Id(10) })
    );
    assert_eq!(f.offset(), 338.0);
    f.view.selected = Some(Id(5));
    f.tree.scene().unwrap();
    f.tree.reconcile(description(&f.view)).unwrap();
    assert_eq!(f.tree.invalidation(), Invalidation::Paint);
    assert_eq!(f.offset(), 168.0); // No layout pass is needed for selection-only updates.
    assert!(f.click(10.0).actions.is_empty());
    f.view.items.reverse();
    f.update();
    assert_eq!(f.tree.focused(), Some(root));
    assert_eq!(f.offset(), 170.0);
}
#[test]
fn wheel_scroll_cancels_armed_selection_and_invalid_descriptions_recover() {
    let mut f = Fixture::new();
    f.tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            position: Some(Point::new(10.0, 20.0)),
        })
        .unwrap();
    let result = f
        .tree
        .dispatch_input(InputEvent::Scroll(ScrollDelta::Pixels {
            x: 0.0,
            y: -100.0,
        }))
        .unwrap();
    assert!(result.handled);
    assert_eq!(f.offset(), 100.0);
    let result = f
        .tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(10.0, 20.0)),
        })
        .unwrap();
    assert!(result.actions.is_empty());
    f.view.items[1].id = Id(1);
    f.tree.reconcile(description(&f.view)).unwrap();
    assert!(
        f.tree
            .layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut f.text,
                Size::new(200.0, 80.0)
            )
            .is_err()
    );
    f.view.items.clear();
    f.view.selected = None;
    f.update();
    assert_eq!(f.offset(), 0.0);
    assert!(f.tree.focused().is_none());
}
fn verify_selected_rows(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    for (selected, y) in [(Id(1), 10), (Id(10), 70)] {
        f.view.selected = Some(selected);
        f.tree.reconcile(description(&f.view)).unwrap();
        for scale in [1u32, 2] {
            let pixels = renderer
                .render(
                    &f.tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(200 * scale, 80 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let offset = (y * scale) as usize * pixels.stride() + (4 * scale * 4) as usize;
            let c = f.view.style.normal.selected_background;
            assert_eq!(&pixels.pixels()[offset..offset + 4], &[c.r, c.g, c.b, c.a]);
        }
    }
    f.view.items.clear();
    f.view.selected = None;
    f.update();
    for scale in [1u32, 2] {
        let pixels = renderer
            .render(
                &f.tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(200 * scale, 80 * scale),
                    scale: scale as f64,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        for row in pixels.pixels().chunks(pixels.stride()) {
            assert!(
                row[..(200 * scale * 4) as usize]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| {
                        let c = f.view.style.normal.background;
                        *pixel == [c.r, c.g, c.b, c.a]
                    })
            );
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn selected_rows_render_and_external_selection_scrolls_without_relayout() {
    verify_selected_rows(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "cpu")]
#[test]
fn scrollbar_thumb_renders_at_scroll_endpoints_and_style_validation_recovers() {
    verify_scrollbar(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn list_box_selection_scrollbar_and_frame_updates_render_gpu() {
    let mut renderer =
        pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap();
    verify_selected_rows(&mut renderer);
    verify_scrollbar(&mut renderer);
}

#[test]
fn width_reflow_expands_long_rows_and_cancels_pending_click() {
    let mut f = Fixture::new();
    f.view.items[0].label = "这是一段需要在较窄列表中自动换行的项目说明".into();
    f.update();
    f.tree.request_focus(Some(f.tree.root())).unwrap();
    f.key(Key::End);
    let wide_offset = f.offset();
    f.key(Key::Home);
    f.tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            position: Some(Point::new(10.0, 20.0)),
        })
        .unwrap();
    let narrow = View::new(f.view.clone()).styled(LayoutStyle {
        width: Dimension::Length(90.0),
        height: Dimension::Length(80.0),
        ..Default::default()
    });
    f.tree.reconcile(narrow).unwrap();
    f.layout();
    let released = f
        .tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(10.0, 20.0)),
        })
        .unwrap();
    assert!(released.actions.is_empty());
    f.key(Key::End);
    assert!(f.offset() > wide_offset);
}

#[test]
fn paging_uses_viewport_geometry_skips_disabled_and_clamps_at_ends() {
    let mut f = Fixture::new();
    f.tree.request_focus(Some(f.tree.root())).unwrap();
    for id in [3, 5, 7, 9, 10] {
        let result = f.key(Key::PageDown);
        assert!(result.handled);
        assert_eq!(
            result.actions[0].action.payload::<ListSelectionChanged>(),
            Some(&ListSelectionChanged { selected: Id(id) })
        );
        f.view.selected = Some(Id(id));
        f.update();
    }
    assert!(f.key(Key::PageDown).actions.is_empty());
    for id in [8, 6, 4, 1] {
        let result = f.key(Key::PageUp);
        assert_eq!(
            result.actions[0].action.payload::<ListSelectionChanged>(),
            Some(&ListSelectionChanged { selected: Id(id) })
        );
        f.view.selected = Some(Id(id));
        f.update();
    }
    assert!(f.key(Key::PageUp).actions.is_empty());
    assert_eq!(f.offset(), 0.0);
}
#[test]
fn paging_past_tall_rows_cancels_space_and_ignores_modified_keys_and_release() {
    let mut f = Fixture::new();
    f.view.items[0].label = "这是一个很长的项目，超过视口时分页仍需移动到下一可用项目。".repeat(6);
    f.update();
    f.tree.request_focus(Some(f.tree.root())).unwrap();
    let input = |pressed, control| {
        InputEvent::Key(KeyInput {
            key: Key::PageDown,
            pressed,
            repeat: false,
            text: None,
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
        })
    };
    assert!(
        f.tree
            .dispatch_input(input(false, false))
            .unwrap()
            .actions
            .is_empty()
    );
    assert!(
        f.tree
            .dispatch_input(input(true, true))
            .unwrap()
            .actions
            .is_empty()
    );
    assert_eq!(f.offset(), 0.0);
    f.key(Key::Space);
    let result = f.key(Key::PageDown);
    assert_eq!(
        result.actions[0].action.payload::<ListSelectionChanged>(),
        Some(&ListSelectionChanged { selected: Id(3) })
    );
    assert!(f.offset() > 80.0);
    let release = InputEvent::Key(KeyInput {
        key: Key::Space,
        pressed: false,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    });
    assert!(f.tree.dispatch_input(release).unwrap().actions.is_empty());
}

#[test]
fn scrollbar_pages_drags_and_releases_without_selecting_underlying_rows() {
    let mut f = Fixture::new();
    let button = |pressed, y| InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(196.0, y)),
    };
    let press = f.tree.dispatch_input(button(true, 70.0)).unwrap();
    assert!(press.handled);
    assert!(press.actions.is_empty());
    assert_eq!(f.offset(), 80.0); // Gaining list focus must not undo track paging.
    assert!(
        f.tree
            .dispatch_input(button(false, 70.0))
            .unwrap()
            .actions
            .is_empty()
    );
    f.key(Key::Home);
    assert_eq!(f.offset(), 0.0);
    f.tree.dispatch_input(button(true, 5.0)).unwrap();
    assert_eq!(f.tree.pointer_capture(), Some(f.tree.root()));
    let drag = f
        .tree
        .dispatch_input(InputEvent::PointerMoved(Point::new(196.0, 200.0)))
        .unwrap();
    assert!(drag.actions.is_empty());
    assert_eq!(f.offset(), 338.0);
    assert!(
        f.tree
            .dispatch_input(button(false, 200.0))
            .unwrap()
            .actions
            .is_empty()
    );
    assert!(f.tree.pointer_capture().is_none());
    assert_eq!(f.view.selected, Some(Id(1)));
    f.key(Key::Home);
    f.tree.dispatch_input(button(true, 5.0)).unwrap();
    f.tree.cancel_input();
    let offset = f.offset();
    f.tree
        .dispatch_input(InputEvent::PointerMoved(Point::new(196.0, 70.0)))
        .unwrap();
    assert_eq!(f.offset(), offset);
    f.view.scrollbar = None;
    f.update();
    assert_eq!(f.click(20.0).actions.len(), 0); // First selected row is still interactive.
}
fn verify_scrollbar(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new();
    let style = f.view.scrollbar.unwrap();
    for (selected, y) in [(Id(1), 5), (Id(10), 75)] {
        f.view.selected = Some(selected);
        f.update();
        for scale in [1u32, 2] {
            let pixels = renderer
                .render(
                    &f.tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(200 * scale, 80 * scale),
                        scale: scale as f64,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let offset = (y * scale) as usize * pixels.stride() + (196 * scale * 4) as usize;
            let c = style.thumb;
            assert_eq!(&pixels.pixels()[offset..offset + 4], &[c.r, c.g, c.b, c.a]);
        }
    }
    f.view.scrollbar.as_mut().unwrap().width = f64::NAN;
    f.tree.reconcile(description(&f.view)).unwrap();
    assert!(
        f.tree
            .layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut f.text,
                Size::new(200.0, 80.0)
            )
            .is_err()
    );
    f.view.scrollbar = Some(style);
    f.update();
}

#[test]
fn character_navigation_cycles_matches_skips_disabled_and_reveals_without_confirmation() {
    let mut f = Fixture::new();
    for (item, label) in f.view.items.iter_mut().zip([
        "Alpha",
        "Apple disabled",
        "Beta",
        "Apricot",
        "Éclair",
        "Zulu",
        "设备甲",
        "设备乙",
        "Åland",
        "alpha last",
    ]) {
        item.label = label.into();
    }
    f.update();
    let root = f.tree.root();
    f.tree.request_focus(Some(root)).unwrap();
    for expected in [Id(4), Id(10), Id(1)] {
        let result = f.key(Key::Character("A".into()));
        if expected == Id(1) {
            assert!(result.actions.is_empty()); // Already confirmed by the application.
        } else {
            assert_eq!(
                result.actions[0].action.payload::<ListSelectionChanged>(),
                Some(&ListSelectionChanged { selected: expected })
            );
            assert!(f.offset() > 0.0);
        }
        assert_eq!(f.view.selected, Some(Id(1))); // Requests do not mutate the model.
        assert_eq!(f.tree.focused(), Some(root));
    }
    assert_eq!(f.offset(), 0.0);
    for (prefix, expected) in [("é", Id(5)), ("设备", Id(7)), ("设备", Id(8)), ("å", Id(9))] {
        let result = f.key(Key::Character(prefix.into()));
        assert_eq!(
            result.actions[0].action.payload::<ListSelectionChanged>(),
            Some(&ListSelectionChanged { selected: expected })
        );
    }
    let offset = f.offset();
    assert!(f.key(Key::Character("missing".into())).actions.is_empty());
    assert_eq!(f.offset(), offset);
    f.view.items.reverse();
    f.update();
    let result = f.key(Key::Character("设".into()));
    assert_eq!(
        result.actions[0].action.payload::<ListSelectionChanged>(),
        Some(&ListSelectionChanged { selected: Id(8) })
    );
}

#[test]
fn character_navigation_filters_input_and_cancels_pending_activation() {
    let mut f = Fixture::new();
    f.view.items[3].label = "Beta".into();
    f.update();
    f.tree.request_focus(Some(f.tree.root())).unwrap();
    for (text, modifiers) in [
        (
            "b",
            Modifiers {
                control: true,
                ..Default::default()
            },
        ),
        (
            "b",
            Modifiers {
                alt: true,
                ..Default::default()
            },
        ),
        (
            "b",
            Modifiers {
                super_key: true,
                ..Default::default()
            },
        ),
        ("", Modifiers::default()),
        ("\n", Modifiers::default()),
        (" ", Modifiers::default()),
    ] {
        let result = f
            .tree
            .dispatch_input(InputEvent::Key(KeyInput {
                key: Key::Character("b".into()),
                text: Some(text.into()),
                pressed: true,
                repeat: false,
                modifiers,
            }))
            .unwrap();
        assert!(result.actions.is_empty());
        assert_eq!(f.offset(), 0.0);
    }
    f.key(Key::Space);
    let result = f
        .tree
        .dispatch_input(InputEvent::Key(KeyInput {
            key: Key::Character("x".into()),
            text: Some("B".into()),
            pressed: true,
            repeat: true,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        }))
        .unwrap();
    assert_eq!(
        result.actions[0].action.payload::<ListSelectionChanged>(),
        Some(&ListSelectionChanged { selected: Id(4) })
    );
    for key in [Key::Space, Key::Character("b".into())] {
        let result = f
            .tree
            .dispatch_input(InputEvent::Key(KeyInput {
                key,
                text: None,
                pressed: false,
                repeat: false,
                modifiers: Default::default(),
            }))
            .unwrap();
        assert!(result.actions.is_empty());
    }
    f.view.disabled = true;
    f.update();
    assert!(f.key(Key::Character("b".into())).actions.is_empty());
}
