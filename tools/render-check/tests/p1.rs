use std::sync::Arc;
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_theme::Theme;
use zui_types::{Point, Size};
use zui_widgets::*;
fn text() -> (zui_text::ParleyText, Vec<String>) {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    (text, families)
}
fn layout(tree: &mut Tree, text: &mut zui_text::ParleyText) {
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        text,
        Size::new(400.0, 240.0),
    )
    .unwrap();
}
fn key(tree: &mut Tree, key: Key, modifiers: Modifiers) -> zui_core::DispatchResult {
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
        modifiers,
    }))
    .unwrap()
}
fn click(tree: &mut Tree, x: f64, y: f64) -> zui_core::DispatchResult {
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(x, y)))
        .unwrap();
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        position: Some(Point::new(x, y)),
        pressed: true,
    })
    .unwrap();
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        position: Some(Point::new(x, y)),
        pressed: false,
    })
    .unwrap()
}
#[test]
fn tree_expands_navigates_and_validates_hidden_nodes() {
    let (mut text, f) = text();
    let mut parent = TreeItem::new(TreeItemId(1), "分组");
    parent.children.push(TreeItem::new(TreeItemId(2), "设备"));
    let mut view = TreeView::themed(
        &Theme::dashboard(),
        vec![parent],
        f,
        Some(TreeItemId(1)),
        ActionId(1),
    );
    let mut tree = Tree::from_view(view.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    assert_eq!(
        key(&mut tree, Key::ArrowRight, Default::default()).actions[0]
            .action
            .payload::<TreeEvent>(),
        Some(&TreeEvent::Expansion {
            id: TreeItemId(1),
            expanded: true
        })
    );
    view.expanded.insert(TreeItemId(1));
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::ArrowRight, Default::default()).actions[0]
            .action
            .payload::<TreeEvent>(),
        Some(&TreeEvent::Selected(TreeItemId(2)))
    );
    view.selected = Some(TreeItemId(2));
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::ArrowLeft, Default::default()).actions[0]
            .action
            .payload::<TreeEvent>(),
        Some(&TreeEvent::Selected(TreeItemId(1)))
    );
    view.selected = Some(TreeItemId(1));
    tree.reconcile(view.build().unwrap()).unwrap();
    layout(&mut tree, &mut text);
    let result = click(&mut tree, 15.0, 20.0);
    assert_eq!(
        result.actions[0].action.payload::<TreeEvent>(),
        Some(&TreeEvent::Expansion {
            id: TreeItemId(1),
            expanded: false
        })
    );
    view.expanded.clear();
    view.items[0].children[0].id = TreeItemId(1);
    assert!(matches!(view.build(), Err(TreeViewError::DuplicateId(_))));
}
#[test]
fn table_selects_rows_and_requests_sort_without_reordering_model() {
    let (mut text, f) = text();
    let mut table = TableView::themed(
        &Theme::dashboard(),
        vec![
            TableColumn::new(TableColumnId(1), "名称", 120.0),
            TableColumn::new(TableColumnId(2), "数值", 100.0),
        ],
        vec![
            TableRow::new(TableRowId(1), vec!["设备一".into(), "30".into()]),
            TableRow::new(TableRowId(2), vec!["设备二".into(), "20".into()]),
        ],
        f,
        ActionId(2),
    );
    let mut view = table.build().unwrap();
    view.style.width = Dimension::Length(220.0);
    view.style.height = Dimension::Length(180.0);
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let result = click(&mut tree, 50.0, 15.0);
    assert_eq!(
        result.actions[0].action.payload::<TableEvent>(),
        Some(&TableEvent::SortRequested(TableSort {
            column: TableColumnId(1),
            direction: SortDirection::Ascending
        }))
    );
    let result = click(&mut tree, 50.0, 55.0);
    assert_eq!(
        result.actions[0].action.payload::<TableEvent>(),
        Some(&TableEvent::Selected(TableRowId(1)))
    );
    assert_eq!(
        key(&mut tree, Key::Enter, Default::default()).actions[0]
            .action
            .payload::<TableEvent>(),
        Some(&TableEvent::Selected(TableRowId(1)))
    );
    assert_eq!(table.rows[0].id, TableRowId(1));
    table.rows[0].cells.pop();
    assert!(matches!(table.build(), Err(TableError::CellCount(_))));
}
fn split_view(split: SplitterView) -> View {
    let mut view = split.build(Container::empty().build(), Container::empty().build());
    view.style.width = Dimension::Length(400.0);
    view.style.height = Dimension::Length(200.0);
    view
}
#[test]
fn splitter_drags_clamps_and_keyboard_changes_ratio() {
    let (mut text, _) = text();
    let mut split =
        SplitterView::themed(&Theme::dashboard(), 0.5, SplitAxis::Horizontal, ActionId(3));
    let mut tree = Tree::from_view(split_view(split), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(200.0, 50.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        position: Some(Point::new(200.0, 50.0)),
        pressed: true,
    })
    .unwrap();
    let result = tree
        .dispatch_input(InputEvent::PointerMoved(Point::new(500.0, 50.0)))
        .unwrap();
    assert_eq!(
        result.actions[0].action.payload::<SplitterChanged>(),
        Some(&SplitterChanged { ratio: 0.9 })
    );
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        position: Some(Point::new(200.0, 50.0)),
        pressed: false,
    })
    .unwrap();
    assert_eq!(
        key(&mut tree, Key::Home, Default::default()).actions[0]
            .action
            .payload::<SplitterChanged>(),
        Some(&SplitterChanged { ratio: 0.1 })
    );
    split.ratio = 0.25;
    tree.reconcile(split_view(split)).unwrap();
    layout(&mut tree, &mut text);
    let first = tree.children(tree.root()).unwrap()[0];
    assert!((tree.layout_box(first).unwrap().bounds.size.width - 98.0).abs() < 0.1);
    split.ratio = f64::NAN;
    tree.reconcile(split_view(split)).unwrap();
    assert!(
        tree.layout_with_text(
            &mut zui_layout::TaffyLayout::default(),
            &mut text,
            Size::new(400.0, 200.0)
        )
        .is_err()
    );
}
#[test]
fn textarea_newlines_navigation_undo_and_wrap() {
    let (mut text, f) = text();
    let mut area = TextAreaView::themed(&Theme::dashboard(), "第一行\n第二行", f, ActionId(4));
    let description = |area: TextAreaView| {
        View::new(area).styled(LayoutStyle {
            width: Dimension::Length(110.0),
            height: Dimension::Length(60.0),
            ..Default::default()
        })
    };
    let mut tree = Tree::from_view(description(area.clone()), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    key(
        &mut tree,
        Key::End,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    let response = key(&mut tree, Key::Enter, Default::default());
    assert_eq!(
        response.actions[0].action.payload::<TextInputEvent>(),
        Some(&TextInputEvent::Changed("第一行\n第二行\n".into()))
    );
    layout(&mut tree, &mut text);
    key(&mut tree, Key::Character("中".into()), Default::default());
    layout(&mut tree, &mut text);
    key(&mut tree, Key::ArrowUp, Default::default());
    tree.update_widget::<TextInput>(tree.root(), Invalidation::None, |w| {
        assert!(w.selection().unwrap().focus < "第一行\n第二行\n".len())
    })
    .unwrap();
    key(
        &mut tree,
        Key::Character("z".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    layout(&mut tree, &mut text);
    tree.update_widget::<TextInput>(tree.root(), Invalidation::None, |w| {
        assert_eq!(w.text(), Some("第一行\n第二行\n"))
    })
    .unwrap();
    area.editor.read_only = true;
    tree.reconcile(description(area)).unwrap();
    layout(&mut tree, &mut text);
    assert!(
        key(&mut tree, Key::Enter, Default::default())
            .actions
            .is_empty()
    );
    tree.scene().unwrap();
}

#[test]
fn textarea_ime_paste_scroll_and_draft_remount_preserve_editing() {
    use zui_api::clipboard::{Clipboard, ClipboardError};
    struct Clip;
    impl Clipboard for Clip {
        fn read_text(&mut self, _: usize) -> Result<String, ClipboardError> {
            Ok("甲\n乙\n丙\n丁".into())
        }
        fn write_text(&mut self, _: &str) -> Result<(), ClipboardError> {
            Ok(())
        }
    }
    let (mut text, f) = text();
    let mut area = TextAreaView::themed(&Theme::dashboard(), "", f, ActionId(8));
    area.editor.draft = Some(TextInputDraft::new());
    let description = |area: TextAreaView| {
        View::new(area).styled(LayoutStyle {
            width: Dimension::Length(100.0),
            height: Dimension::Length(35.0),
            ..Default::default()
        })
    };
    let mut tree = Tree::from_view(description(area.clone()), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    tree.dispatch_input_with_services(
        InputEvent::Key(KeyInput {
            key: Key::Character("v".into()),
            text: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        }),
        &mut EventServices {
            clipboard: &mut Clip,
        },
    )
    .unwrap();
    layout(&mut tree, &mut text);
    tree.update_widget::<TextInput>(tree.root(), Invalidation::None, |w| {
        assert_eq!(w.text(), Some("甲\n乙\n丙\n丁"));
        assert!(w.vertical_scroll() > 0.0);
    })
    .unwrap();
    tree.dispatch_input(InputEvent::ImePreedit {
        text: "中".into(),
        cursor: Some((3, 3)),
    })
    .unwrap();
    layout(&mut tree, &mut text);
    assert!(
        key(&mut tree, Key::Enter, Default::default())
            .actions
            .is_empty()
    );
    tree.dispatch_input(InputEvent::ImeCommit("中文".into()))
        .unwrap();
    layout(&mut tree, &mut text);
    tree.dispatch_input(InputEvent::PointerMoved(Point::new(10.0, 10.0)))
        .unwrap();
    let response = tree
        .dispatch_input(InputEvent::Scroll(ScrollDelta::Lines { x: 0.0, y: 10.0 }))
        .unwrap();
    assert!(response.handled);
    tree.update_widget::<TextInput>(tree.root(), Invalidation::None, |w| {
        assert_eq!(w.vertical_scroll(), 0.0)
    })
    .unwrap();
    drop(tree);
    let mut tree = Tree::from_view(description(area), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    tree.update_widget::<TextInput>(tree.root(), Invalidation::None, |w| {
        assert_eq!(w.text(), Some("甲\n乙\n丙\n丁中文"))
    })
    .unwrap();
    key(
        &mut tree,
        Key::Character("z".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    layout(&mut tree, &mut text);
    tree.update_widget::<TextInput>(tree.root(), Invalidation::None, |w| {
        assert_eq!(w.text(), Some("甲\n乙\n丙\n丁"))
    })
    .unwrap();
}

fn verify_split_pixels(renderer: &mut impl zui_paint::OffscreenRenderer) {
    let (mut text, _) = text();
    let split = SplitterView::themed(&Theme::dashboard(), 0.5, SplitAxis::Horizontal, ActionId(9));
    let mut tree = Tree::from_view(split_view(split), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    for scale in [1, 2] {
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                zui_paint::Frame {
                    extent: zui_types::Extent::new(400 * scale, 200 * scale),
                    scale: scale as f64,
                    background: zui_types::Color::BLACK,
                },
            )
            .unwrap();
        let at = |x: u32, y: u32| {
            let i = (y * scale) as usize * pixels.stride() + (x * scale * 4) as usize;
            &pixels.pixels()[i..i + 4]
        };
        assert_eq!(
            at(200, 100),
            &[split.color.r, split.color.g, split.color.b, 255]
        );
        assert_eq!(at(100, 100), &[0, 0, 0, 255]);
    }
}
#[cfg(feature = "cpu")]
#[test]
fn splitter_pixels_cpu() {
    verify_split_pixels(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a GPU adapter"]
fn splitter_pixels_gpu() {
    verify_split_pixels(
        &mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap(),
    );
}

#[test]
fn multiline_selection_collapses_by_line_before_horizontal_position() {
    let (mut text, f) = text();
    let area = TextAreaView::themed(&Theme::dashboard(), "abcdef\nx", f, ActionId(1));
    let mut tree = Tree::from_view(
        View::new(area).styled(LayoutStyle {
            width: Dimension::Length(240.0),
            height: Dimension::Length(100.0),
            ..Default::default()
        }),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    key(
        &mut tree,
        Key::Home,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    for _ in 0..3 {
        key(&mut tree, Key::ArrowRight, Default::default());
    }
    key(
        &mut tree,
        Key::End,
        Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        },
    );
    key(&mut tree, Key::ArrowRight, Default::default());
    tree.update_widget::<TextInput>(tree.root(), Invalidation::None, |w| {
        assert_eq!(w.selection().unwrap().focus, "abcdef\nx".len())
    })
    .unwrap();
}
