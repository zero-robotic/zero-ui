use std::{collections::BTreeSet, sync::Arc};
use zui_api::{action::ActionId, event::*, layout::Dimension, view::View};
use zui_core::Tree;
use zui_theme::Theme;
use zui_types::{Color, Point, Size};
use zui_widgets::*;
fn text() -> (zui_text::ParleyText, Vec<String>) {
    let mut text = zui_text::ParleyText::default();
    let f = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    (text, f)
}
fn layout(tree: &mut Tree, text: &mut zui_text::ParleyText) {
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        text,
        Size::new(640.0, 480.0),
    )
    .unwrap();
}
fn key(tree: &mut Tree, key: Key, control: bool) -> zui_core::DispatchResult {
    let text = if let Key::Character(s) = &key {
        Some(s.clone())
    } else {
        None
    };
    tree.dispatch_input(InputEvent::Key(KeyInput {
        key,
        text,
        pressed: true,
        repeat: false,
        modifiers: Modifiers {
            control,
            ..Default::default()
        },
    }))
    .unwrap()
}
fn click(tree: &mut Tree, x: f64, y: f64) -> zui_core::DispatchResult {
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(x, y)))
        .unwrap();
    let down = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            position: Some(Point::new(x, y)),
        })
        .unwrap();
    let mut up = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(x, y)),
        })
        .unwrap();
    up.actions.splice(0..0, down.actions);
    up
}
fn all_nodes(tree: &Tree, root: zui_types::NodeId) -> Vec<zui_types::NodeId> {
    let mut result = vec![root];
    for child in tree.children(root).unwrap() {
        result.extend(all_nodes(tree, *child));
    }
    result
}
#[test]
fn combo_search_custom_input_and_filtered_multi_selection() {
    let (mut text, f) = text();
    let mut combo = ComboBoxView::themed(
        &Theme::dark(),
        vec![
            ListItem::new(ListItemId(1), "Alpha"),
            ListItem::new(ListItemId(2), "Beta"),
        ],
        None,
        f,
        ActionId(1),
    );
    combo.query = "ALP".into();
    combo.selection = Some(BTreeSet::from([ListItemId(2)]));
    assert_eq!(combo.filtered_items().len(), 1);
    combo.allow_custom = true;
    let mut tree = Tree::from_view(
        combo.popup(
            zui_types::Rect::new(0.0, 0.0, 220.0, 0.0),
            220.0,
            ActionId(2),
        ),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    let result = key(&mut tree, Key::Enter, false);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<ComboBoxEvent>()
                == Some(&ComboBoxEvent::Submitted("ALP".into())))
    );
    key(&mut tree, Key::Tab, false);
    key(&mut tree, Key::Space, false);
    let result = tree
        .dispatch_input(InputEvent::Key(KeyInput {
            key: Key::Space,
            text: None,
            pressed: false,
            repeat: false,
            modifiers: Default::default(),
        }))
        .unwrap();
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<ComboBoxEvent>()
                == Some(&ComboBoxEvent::SelectionChanged(BTreeSet::from([
                    ListItemId(1),
                    ListItemId(2)
                ]))))
    );
    combo.allow_custom = false;
    tree.reconcile(combo.popup(
        zui_types::Rect::new(0.0, 0.0, 220.0, 0.0),
        220.0,
        ActionId(2),
    ))
    .unwrap();
    layout(&mut tree, &mut text);
}
#[test]
fn tree_lazy_requests_multi_selection_and_drag_moves_are_controlled() {
    let (mut text, f) = text();
    let mut lazy = TreeItem::new(TreeItemId(1), "Load");
    lazy.children_state = TreeChildrenState::Unloaded;
    let mut view = TreeView::themed(
        &Theme::dark(),
        vec![lazy, TreeItem::new(TreeItemId(2), "Target")],
        f,
        Some(TreeItemId(1)),
        ActionId(3),
    );
    let mut tree = Tree::from_view(view.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    let result = key(&mut tree, Key::ArrowRight, false);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TreeLoadRequested>()
                == Some(&TreeLoadRequested { id: TreeItemId(1) }))
    );
    view.items[0].children_state = TreeChildrenState::Loading;
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    assert!(
        !key(&mut tree, Key::ArrowRight, false)
            .actions
            .iter()
            .any(|a| a.action.payload::<TreeLoadRequested>().is_some())
    );
    view.items[0].children_state = TreeChildrenState::Loaded;
    view.selection = Some(BTreeSet::from([TreeItemId(2)]));
    view.draggable = true;
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    let result = click(&mut tree, 40.0, 15.0);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TreeSelectionChanged>().is_some())
    );
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(40.0, 15.0)),
    })
    .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(40.0, 55.0)))
        .unwrap();
    let result = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(40.0, 55.0)),
        })
        .unwrap();
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TreeDropRequested>()
                == Some(&TreeDropRequested {
                    sources: vec![TreeItemId(1)],
                    target: TreeItemId(2)
                }))
    );
}
#[test]
fn tabs_close_drag_reorder_and_detach_requests() {
    let (mut text, f) = text();
    let mut view = TabsView::themed(
        &Theme::dark(),
        vec![
            TabOption::new(TabId(1), "One"),
            TabOption::new(TabId(2), "Two"),
        ],
        f,
        Some(TabId(1)),
        ActionId(4),
    );
    view.compact = false; // Legacy equal-width mode retains its hit geometry.
    view.closable.insert(TabId(1));
    view.reorderable = true;
    view.detachable = true;
    view.layout.width = Dimension::Length(400.0);
    let mut tree =
        Tree::from_view(view.build(|_| row([]).build()).unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let result = click(&mut tree, 190.0, 15.0);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TabCloseRequested>()
                == Some(&TabCloseRequested { id: TabId(1) }))
    );
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(50.0, 15.0)),
    })
    .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(270.0, 15.0)))
        .unwrap();
    let result = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(270.0, 15.0)),
        })
        .unwrap();
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TabReorderRequested>()
                == Some(&TabReorderRequested {
                    id: TabId(1),
                    to_index: 1
                }))
    );
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(50.0, 15.0)),
    })
    .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(80.0, 150.0)))
        .unwrap();
    let result = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(80.0, 150.0)),
        })
        .unwrap();
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TabDetachRequested>().is_some())
    );
}
#[test]
fn virtual_grid_bounds_realized_nodes_and_accepts_custom_cells() {
    let (mut text, f) = text();
    let table = TableView::themed(
        &Theme::dark(),
        vec![TableColumn::new(TableColumnId(1), "Value", 200.0)],
        (0..10_000)
            .map(|id| TableRow::new(TableRowId(id), vec![format!("row {id}")]))
            .collect(),
        f,
        ActionId(5),
    );
    let mut grid = DataGridView::new(table);
    grid.virtual_rows = Some(DataGridVirtualRows {
        estimated_height: 40.0,
        viewport_height: 120.0,
        revision: 1,
    });
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = calls.clone();
    grid.cell_factory = Some(std::rc::Rc::new(move |_, _, _| {
        count.set(count.get() + 1);
        row([])
            .style(|s| s.height = Dimension::Length(28.0))
            .build()
    }));
    let mut tree = Tree::from_view(grid.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    assert!(all_nodes(&tree, tree.root()).len() < 100);
    assert!(calls.get() < 100);
    grid.virtual_rows.as_mut().unwrap().viewport_height = f32::NAN;
    assert!(matches!(grid.build(), Err(DataGridError::InvalidViewport)));
}
#[test]
fn menu_hover_opens_submenu_and_separator_never_activates() {
    let (mut text, f) = text();
    let mut branch = MenuItem::new(MenuId(1), "Branch");
    branch.children.push(MenuItem::new(MenuId(2), "Child"));
    let menu = MenuView::themed(
        &Theme::dark(),
        vec![MenuItem::separator(MenuId(0)), branch],
        f,
        ActionId(6),
    );
    let mut tree = Tree::from_view(
        menu.popup(PopupPlacement::Point(Point::ZERO), ActionId(7))
            .unwrap(),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    let result = click(&mut tree, 30.0, 10.0);
    assert!(
        !result
            .actions
            .iter()
            .any(|a| a.action.payload::<MenuEvent>().is_some())
    );
    let result = tree
        .dispatch_input(InputEvent::PointerMoved(Point::new(30.0, 70.0)))
        .unwrap();
    assert!(result.actions.is_empty());
    let mut actions = Vec::new();
    tree.advance_animations_with_actions(std::time::Duration::ZERO, &mut actions)
        .unwrap();
    assert!(actions.is_empty());
    tree.advance_animations_with_actions(menu.behavior.hover_delay, &mut actions)
        .unwrap();
    assert!(actions.iter().any(|a| a.action.payload::<MenuEvent>() == Some(&MenuEvent::OpenPath(vec![MenuId(1)]))));
}

#[test]
fn hsv_conversion_plane_keyboard_and_alpha() {
    for color in [
        Color::rgb(255, 0, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(0, 0, 255),
        Color::rgb(12, 64, 123),
        Color::WHITE,
        Color::BLACK,
    ] {
        assert_eq!(HsvColor::from_color(color).to_color(255), Some(color));
    }
    assert!(HsvColor::new(f32::NAN, 0.0, 0.0).is_none());
    let (mut text, _) = text();
    let hsv = HsvColor::new(120.0, 1.0, 1.0).unwrap();
    let mut tree = Tree::from_view(
        View::new(HsvPickerView {
            hsv,
            alpha: 128,
            disabled: false,
            action: ActionId(8),
        }),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    let result = key(&mut tree, Key::ArrowDown, false);
    let changed = result
        .actions
        .iter()
        .find_map(|a| a.action.payload::<HsvChanged>())
        .unwrap();
    assert_eq!(changed.hsv.hue, 120.0);
    assert!(changed.hsv.value < 1.0);
    assert_eq!(changed.color.a, 128);
    let result = click(&mut tree, 112.0, 64.0);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<ColorChanged>().is_some())
    );
}
#[test]
fn virtual_tree_keeps_focus_and_navigates_to_unrealized_last_row() {
    let (mut text, f) = text();
    let mut view = TreeView::themed(
        &Theme::dark(),
        (0..10_000)
            .map(|id| TreeItem::new(TreeItemId(id), format!("Node {id}")))
            .collect(),
        f,
        Some(TreeItemId(0)),
        ActionId(9),
    );
    let mut tree =
        Tree::from_view(view.build_virtual(56.0, 168.0).unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    assert!(all_nodes(&tree, tree.root()).len() < 30);
    tree.request_focus(Some(tree.root())).unwrap();
    let result = key(&mut tree, Key::End, false);
    assert!(
        result.actions.iter().any(
            |a| a.action.payload::<TreeEvent>() == Some(&TreeEvent::Selected(TreeItemId(9999)))
        )
    );
    view.selected = Some(TreeItemId(9999));
    tree.reconcile(view.build_virtual(56.0, 168.0).unwrap())
        .unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.focused(), Some(tree.root()));
    assert!(all_nodes(&tree, tree.root()).len() < 30);
    let result = key(&mut tree, Key::ArrowUp, false);
    assert!(
        result.actions.iter().any(
            |a| a.action.payload::<TreeEvent>() == Some(&TreeEvent::Selected(TreeItemId(9998)))
        )
    );
    view.draggable = true;
    assert!(matches!(
        view.build_virtual(56.0, 168.0),
        Err(TreeViewError::VirtualDragUnsupported)
    ));
}
#[test]
fn grid_draft_survives_row_recycling() {
    let (mut text, f) = text();
    let record = TableRow::new(TableRowId(1), vec!["10".into()]);
    let table = TableView::themed(
        &Theme::dark(),
        vec![TableColumn::new(TableColumnId(1), "Value", 200.0)],
        vec![record.clone()],
        f,
        ActionId(10),
    );
    let mut grid = DataGridView::new(table);
    grid.editable_columns.insert(TableColumnId(1));
    let cell = CellAddress {
        row: TableRowId(1),
        column: TableColumnId(1),
    };
    let draft = TextInputDraft::new();
    grid.drafts.insert(cell, draft.clone());
    let mut tree = Tree::from_view(grid.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    click(&mut tree, 40.0, 55.0);
    key(&mut tree, Key::Character("a".into()), true);
    key(&mut tree, Key::Character("unsaved".into()), false);
    grid.table.rows.clear();
    tree.reconcile(grid.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    assert!(!draft.is_attached());
    grid.table.rows.push(record);
    tree.reconcile(grid.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    click(&mut tree, 40.0, 55.0);
    let result = key(&mut tree, Key::Enter, false);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<DataGridEvent>()
                == Some(&DataGridEvent::CellEdited {
                    cell,
                    value: "unsaved".into()
                }))
    );
}
fn verify_hsv_pixels(renderer: &mut impl zui_paint::OffscreenRenderer) {
    let (mut text, _) = text();
    let mut tree = Tree::from_view(
        View::new(HsvPickerView {
            hsv: HsvColor::new(120.0, 1.0, 1.0).unwrap(),
            alpha: 255,
            disabled: false,
            action: ActionId(11),
        }),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    for scale in [1, 2] {
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                zui_paint::Frame {
                    extent: zui_types::Extent::new(224 * scale, 156 * scale),
                    scale: scale as f64,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let offset = (2 * scale) as usize * pixels.stride() + (210 * scale * 4) as usize;
        let actual = &pixels.pixels()[offset..offset + 4];
        for (a, b) in actual.iter().zip([8u8, 255, 8, 255]) {
            assert!(a.abs_diff(b) <= 2, "{actual:?}");
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn hsv_pixels_cpu() {
    verify_hsv_pixels(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "Requires a real GPU adapter"]
fn hsv_pixels_gpu() {
    verify_hsv_pixels(
        &mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap(),
    );
}

#[test]
fn tree_label_double_click_toggles_and_expires() {
    use std::time::Duration;
    let (mut text, f) = text();
    let mut branch = TreeItem::new(TreeItemId(1), "Branch");
    branch.children_state = TreeChildrenState::Unloaded;
    let mut view = TreeView::themed(
        &Theme::dark(),
        vec![branch, TreeItem::new(TreeItemId(2), "Leaf")],
        f,
        None,
        ActionId(1),
    );
    view.draggable = true;
    let mut tree = Tree::from_view(view.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let expansion = |result: &zui_core::DispatchResult| {
        result
            .actions
            .iter()
            .find_map(|a| match a.action.payload::<TreeEvent>() {
                Some(TreeEvent::Expansion { expanded, .. }) => Some(*expanded),
                _ => None,
            })
    };
    assert_eq!(expansion(&click(&mut tree, 60.0, 15.0)), None);
    tree.advance_animations(Duration::ZERO).unwrap();
    // Selection reconciliation must preserve the first click.
    view.selected = Some(TreeItemId(1));
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    tree.advance_animations(Duration::from_millis(100)).unwrap();
    let result = click(&mut tree, 60.0, 15.0);
    assert_eq!(expansion(&result), Some(true));
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TreeLoadRequested>().is_some())
    );
    view.expanded.insert(TreeItemId(1));
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(expansion(&click(&mut tree, 60.0, 15.0)), None);
    tree.advance_animations(Duration::from_millis(200)).unwrap();
    assert_eq!(expansion(&click(&mut tree, 60.0, 15.0)), Some(false));
    assert_eq!(expansion(&click(&mut tree, 60.0, 15.0)), None);
    tree.advance_animations(Duration::from_millis(300)).unwrap();
    tree.advance_animations(Duration::from_millis(800)).unwrap();
    assert_eq!(expansion(&click(&mut tree, 60.0, 15.0)), None);
    // Platform interval overrides reset any pending gesture; zero disables it.
    view.double_click_interval = Duration::ZERO;
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(expansion(&click(&mut tree, 60.0, 15.0)), None);
    assert_eq!(expansion(&click(&mut tree, 60.0, 15.0)), None);
    assert!(
        tree.advance_animations(Duration::from_millis(900))
            .unwrap()
            .next_frame
            .is_none()
    );
    // Arrow clicks still toggle immediately; leaves never expand.
    assert_eq!(expansion(&click(&mut tree, 18.0, 15.0)), Some(false));
    assert_eq!(expansion(&click(&mut tree, 60.0, 55.0)), None);
    assert_eq!(expansion(&click(&mut tree, 60.0, 55.0)), None);
}

#[test]
fn tree_double_click_is_cancelled_by_drag_scroll_and_disabled_state() {
    let (mut text, f) = text();
    let mut branch = TreeItem::new(TreeItemId(1), "Branch");
    branch.children_state = TreeChildrenState::Unloaded;
    let mut view = TreeView::themed(&Theme::dark(), vec![branch], f, None, ActionId(1));
    view.draggable = true;
    let mut tree = Tree::from_view(view.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let no_expansion = |result: zui_core::DispatchResult| {
        assert!(!result.actions.iter().any(|a| matches!(
            a.action.payload::<TreeEvent>(),
            Some(TreeEvent::Expansion { .. })
        )))
    };
    no_expansion(click(&mut tree, 60.0, 15.0));
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(60.0, 15.0)),
    })
    .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(80.0, 15.0)))
        .unwrap();
    no_expansion(
        tree.dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(60.0, 15.0)),
        })
        .unwrap(),
    );
    no_expansion(click(&mut tree, 60.0, 15.0));
    tree.dispatch_input(InputEvent::Scroll(ScrollDelta::Lines { x: 0.0, y: 1.0 }))
        .unwrap();
    no_expansion(click(&mut tree, 60.0, 15.0));
    view.items[0].disabled = true;
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    no_expansion(click(&mut tree, 60.0, 15.0));
    no_expansion(click(&mut tree, 60.0, 15.0));
}

#[test]
fn compact_tabs_use_label_widths_for_selection_close_and_reorder() {
    use zui_api::text::{TextRequest, TextService, TextWidth};
    let (mut text, f) = text();
    let mut view = TabsView::themed(
        &Theme::desktop_dark(),
        vec![
            TabOption::new(TabId(1), "A"),
            TabOption::new(TabId(2), "设备运行详情"),
        ],
        f,
        Some(TabId(1)),
        ActionId(4),
    );
    view.closable = BTreeSet::from([TabId(1), TabId(2)]);
    view.reorderable = true;
    view.layout.width = Dimension::Length(600.0);
    let widths: Vec<_> = view
        .tabs
        .iter()
        .map(|tab| {
            text.layout(TextRequest {
                text: &tab.label,
                style: &view.text_style,
                width: TextWidth::Unbounded,
            })
            .unwrap()
            .size
            .width
                + 44.0
        })
        .collect();
    assert!(widths[1] > widths[0]);
    let second = widths[0] + f64::from(view.header_style.gap) + widths[1] / 2.0;
    let mut tree =
        Tree::from_view(view.build(|_| row([]).build()).unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    assert!(click(&mut tree, 550.0, 15.0).actions.is_empty());
    let result = click(&mut tree, second, 15.0);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TabChanged>() == Some(&TabChanged { selected: TabId(2) }))
    );
    let result = click(&mut tree, widths[0] - 8.0, 15.0);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TabCloseRequested>()
                == Some(&TabCloseRequested { id: TabId(1) }))
    );
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(12.0, 15.0)),
    })
    .unwrap();
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(second, 15.0)))
        .unwrap();
    let result = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(second, 15.0)),
        })
        .unwrap();
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TabReorderRequested>()
                == Some(&TabReorderRequested {
                    id: TabId(1),
                    to_index: 1
                }))
    );
    let result = key(&mut tree, Key::End, false);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<TabChanged>() == Some(&TabChanged { selected: TabId(2) }))
    );
}

#[cfg(feature = "cpu")]
#[test]
fn tabs_share_one_divider_without_independent_cell_borders() {
    use zui_paint::{Frame, OffscreenRenderer};
    use zui_types::Extent;
    let (mut text, f) = text();
    let theme = Theme::desktop_dark();
    let mut view = TabsView::themed(
        &theme,
        vec![TabOption::new(TabId(1), "A"), TabOption::new(TabId(2), "B")],
        f,
        Some(TabId(1)),
        ActionId(1),
    );
    view.compact = false;
    view.layout.width = Dimension::Length(300.0);
    let mut tree =
        Tree::from_view(view.build(|_| row([]).build()).unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    for scale in [1_u32, 2] {
        let pixels = zui_render_cpu::CpuRenderer::default()
            .render(
                &tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(300 * scale, 80 * scale),
                    scale: f64::from(scale),
                    background: Color::BLACK,
                },
            )
            .unwrap();
        for (x, color) in [
            (149, theme.selection_background()),
            (150, theme.color(zui_theme::ColorRole::Border)),
            (151, theme.color(zui_theme::ColorRole::Surface)),
        ] {
            let i = (2 * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
            assert_eq!(
                &pixels.pixels()[i..i + 4],
                &[color.r, color.g, color.b, color.a]
            );
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn tree_drop_target_is_visible_only_during_a_valid_drag() {
    use zui_paint::{Frame, OffscreenRenderer};
    use zui_types::Extent;
    let (mut text, f) = text();
    let mut view = TreeView::themed(
        &Theme::dark(),
        vec![
            TreeItem::new(TreeItemId(1), "Source"),
            TreeItem::new(TreeItemId(2), "Target"),
        ],
        f,
        None,
        ActionId(3),
    );
    view.draggable = true;
    let marker = Color::rgb(255, 0, 255);
    view.list.style.focus_ring = marker;
    let mut tree = Tree::from_view(view.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let marked = |tree: &mut Tree| {
        let pixels = zui_render_cpu::CpuRenderer::default()
            .render(
                &tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(640, 480),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let i = 55 * pixels.stride() + 2 * 4;
        pixels.pixels()[i..i + 4] == [marker.r, marker.g, marker.b, marker.a]
    };
    let press = |tree: &mut Tree| {
        tree.dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: true,
            position: Some(Point::new(40.0, 15.0)),
        })
        .unwrap();
    };
    assert!(!marked(&mut tree));
    press(&mut tree);
    assert!(!marked(&mut tree));
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(40.0, 55.0)))
        .unwrap();
    assert!(marked(&mut tree));
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(700.0, 55.0)))
        .unwrap();
    assert!(!marked(&mut tree));
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(40.0, 55.0)))
        .unwrap();
    assert!(marked(&mut tree));
    key(&mut tree, Key::Escape, false);
    assert!(!marked(&mut tree));
    press(&mut tree);
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(40.0, 55.0)))
        .unwrap();
    assert!(marked(&mut tree));
    let released = tree
        .dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed: false,
            position: Some(Point::new(40.0, 55.0)),
        })
        .unwrap();
    assert!(
        released
            .actions
            .iter()
            .any(|a| a.action.payload::<TreeDropRequested>().is_some())
    );
    assert!(!marked(&mut tree));
    view.items[1].disabled = true;
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    press(&mut tree);
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(40.0, 55.0)))
        .unwrap();
    assert!(!marked(&mut tree));
}
