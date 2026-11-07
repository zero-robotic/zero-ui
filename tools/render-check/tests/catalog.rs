use std::{sync::Arc, time::Duration};
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

#[test]
fn calendar_leap_years_boundaries_ranges_and_day_actions() {
    assert!(Date::new(2000, 2, 29).is_some());
    assert!(Date::new(1900, 2, 29).is_none());
    assert!(Date::new(0, 1, 1).is_none());
    assert_eq!(Date::new(2026, 11, 1).unwrap().weekday(), Some(6));
    assert_eq!(
        Date::new(2025, 1, 31).unwrap().shift_month(1),
        Date::new(2025, 2, 28)
    );
    assert!(Date::new(9999, 12, 1).unwrap().shift_month(1).is_none());
    assert!(
        Date::new(2025, 1, 1)
            .unwrap()
            .shift_month(i32::MAX)
            .is_none()
    );
    let (mut text, f) = text();
    let mut calendar = CalendarView::themed(&Theme::dashboard(), 2026, 11, f, ActionId(1));
    calendar.min = Date::new(2026, 11, 2);
    calendar.max = Date::new(2026, 11, 20);
    let mut tree = Tree::from_view(calendar.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let grid = tree.children(tree.root()).unwrap()[1];
    let day1 = tree.children(grid).unwrap()[13];
    let day2 = tree.children(grid).unwrap()[14];
    assert!(tree.request_focus(Some(day1)).is_err());
    tree.request_focus(Some(day2)).unwrap();
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<DateEvent>(),
        Some(&DateEvent::Selected(Date::new(2026, 11, 2).unwrap()))
    );
    calendar.month = 13;
    assert!(calendar.build().is_err());
}
#[test]
fn typed_time_rejects_invalid_input_and_escape_restores_confirmed_value() {
    assert_eq!(Time::parse("23:59:59"), Time::new(23, 59, 59));
    for invalid in ["24:00:00", "12:60:00", "1:02:03", "中:00:00", "12:00:60"] {
        assert!(Time::parse(invalid).is_none());
    }
    let (mut text, f) = text();
    let view = TimePickerView::themed(
        &Theme::dashboard(),
        Time::new(9, 30, 0).unwrap(),
        f,
        ActionId(2),
    );
    let mut tree = Tree::from_view(view.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    tree.request_focus(Some(tree.root())).unwrap();
    key(&mut tree, Key::Character("a".into()), true);
    key(&mut tree, Key::Character("99:00:00".into()), false);
    layout(&mut tree, &mut text);
    assert!(key(&mut tree, Key::Enter, false).actions.is_empty());
    key(&mut tree, Key::Escape, false);
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<TimeChanged>(),
        Some(&TimeChanged {
            time: Time::new(9, 30, 0).unwrap()
        })
    );
    key(&mut tree, Key::Character("a".into()), true);
    key(&mut tree, Key::Character("12:45:30".into()), false);
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<TimeChanged>(),
        Some(&TimeChanged {
            time: Time::new(12, 45, 30).unwrap()
        })
    );
}
#[test]
fn color_hex_and_channels_submit_typed_values() {
    assert_eq!(
        parse_hex_color("#ff008080"),
        Some(Color::rgba(255, 0, 128, 128))
    );
    assert_eq!(parse_hex_color("00ff00"), Some(Color::rgb(0, 255, 0)));
    for s in ["#fff", "#gg0000", "中000000"] {
        assert!(parse_hex_color(s).is_none());
    }
    let (mut text, f) = text();
    let view = ColorPickerView::themed(&Theme::dashboard(), Color::BLACK, f, ActionId(3));
    let mut tree = Tree::from_view(view.build(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let hex = tree.children(tree.root()).unwrap()[1];
    tree.request_focus(Some(hex)).unwrap();
    key(&mut tree, Key::Character("a".into()), true);
    key(&mut tree, Key::Character("#11223344".into()), false);
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<ColorChanged>(),
        Some(&ColorChanged {
            color: Color::rgba(17, 34, 51, 68)
        })
    );
}
#[test]
fn pagination_is_bounded_and_breadcrumb_current_item_does_not_navigate() {
    let (mut text, f) = text();
    let mut tree = Tree::from_view(
        PaginationView::themed(&Theme::dashboard(), 1, u64::MAX, f.clone(), ActionId(4))
            .build()
            .unwrap(),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.children(tree.root()).unwrap().len(), 9);
    let next = tree.children(tree.root()).unwrap()[7];
    tree.request_focus(Some(next)).unwrap();
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<PageChanged>(),
        Some(&PageChanged { page: 2 })
    );
    assert!(
        PaginationView::themed(&Theme::dashboard(), 0, 0, f.clone(), ActionId(4))
            .build()
            .is_err()
    );
    let crumbs = BreadcrumbView::themed(
        &Theme::dashboard(),
        vec![BreadcrumbItem::new(1, "根"), BreadcrumbItem::new(2, "目录")],
        f,
        ActionId(5),
    );
    let mut tree = Tree::from_view(crumbs.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let children = tree.children(tree.root()).unwrap().to_vec();
    assert!(tree.request_focus(Some(children[2])).is_err());
    tree.request_focus(Some(children[0])).unwrap();
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<BreadcrumbSelected>(),
        Some(&BreadcrumbSelected { id: 1 })
    );
}
#[test]
fn toast_expires_without_focus_scope_and_revision_restarts_timer() {
    let (mut text, f) = text();
    let mut view = ToastView::new(NotificationView::themed(
        &Theme::dashboard(),
        1,
        "完成",
        "后台操作完成",
        f,
    ));
    view.duration = Duration::from_secs(2);
    let mut tree = Tree::from_view(view.clone().build(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    assert!(tree.focused().is_none());
    assert_eq!(
        tree.advance_animations(Duration::from_secs(5))
            .unwrap()
            .next_frame,
        Some(Duration::from_secs(7))
    );
    tree.advance_animations(Duration::from_secs(7)).unwrap();
    layout(&mut tree, &mut text);
    assert!(tree.children(tree.root()).unwrap().is_empty());
    view.revision = 1;
    tree.reconcile(view.build()).unwrap();
    layout(&mut tree, &mut text);
    assert_eq!(tree.children(tree.root()).unwrap().len(), 1);
    assert_eq!(
        tree.advance_animations(Duration::from_secs(8))
            .unwrap()
            .next_frame,
        Some(Duration::from_secs(10))
    );
}
#[test]
fn form_aligns_fields_and_renders_validation_without_intercepting_editors() {
    let (mut text, f) = text();
    let make = |label: &str| {
        View::new(TextInputView::themed(
            &Theme::dashboard(),
            label,
            f.clone(),
            ActionId(6),
        ))
    };
    let mut first = FormField::new("one", "名称", make("abc"));
    first.required = true;
    first.error = Some("不能为空".into());
    let form = FormView::themed(&Theme::dashboard(), f.clone()).build(vec![
        first,
        FormField::new("two", "较长的标签", make("def")),
    ]);
    let mut tree = Tree::from_view(
        GroupBoxView::themed(&Theme::dashboard(), "设置", f).build(form),
        Default::default(),
    )
    .unwrap();
    layout(&mut tree, &mut text);
    tree.scene().unwrap();
    let body = tree.children(tree.root()).unwrap()[0];
    let form = tree.children(body).unwrap()[1];
    let rows = tree.children(form).unwrap();
    let first_editor = tree.children(rows[0]).unwrap()[1];
    let second_editor = tree.children(rows[1]).unwrap()[1];
    assert_eq!(
        tree.layout_box(first_editor).unwrap().bounds.origin.x,
        132.0
    );
    assert_eq!(
        tree.layout_box(first_editor).unwrap().bounds.origin.x,
        tree.layout_box(second_editor).unwrap().bounds.origin.x
    );
}
#[test]
fn file_picker_filters_entries_and_requests_navigation_without_io() {
    let (mut text, f) = text();
    let mut picker = FilePickerView::themed(
        &Theme::dashboard(),
        "/demo",
        vec![
            FileEntry::new(FileEntryId(1), "config", true),
            FileEntry::new(FileEntryId(2), "a.txt", false),
            FileEntry::new(FileEntryId(3), "b.png", false),
            FileEntry::new(FileEntryId(4), ".hidden.txt", false),
        ],
        f,
        ActionId(7),
    );
    picker.extensions = vec!["txt".into()];
    picker.selected = Some(FileEntryId(1));
    let mut tree = Tree::from_view(picker.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let list = tree.children(tree.root()).unwrap()[1];
    tree.request_focus(Some(list)).unwrap();
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<FilePickerEvent>(),
        Some(&FilePickerEvent::EnterDirectory(FileEntryId(1)))
    );
    let response = key(&mut tree, Key::End, false);
    assert_eq!(
        response.actions[0].action.payload::<FilePickerEvent>(),
        Some(&FilePickerEvent::Selected(FileEntryId(2)))
    );
    assert!(!FilePickerView::valid_filename("../a.txt"));
    assert!(!FilePickerView::valid_filename("a\\b"));
    picker.mode = FilePickerMode::SaveFile;
    picker.filename = "../bad".into();
    let mut tree = Tree::from_view(picker.build().unwrap(), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let footer = *tree.children(tree.root()).unwrap().last().unwrap();
    let save = *tree.children(footer).unwrap().last().unwrap();
    assert!(tree.request_focus(Some(save)).is_err());
}
fn grid(f: Vec<String>) -> DataGridView {
    let table = TableView::themed(
        &Theme::dashboard(),
        vec![
            TableColumn::new(TableColumnId(1), "名称", 120.0),
            TableColumn::new(TableColumnId(2), "数值", 120.0),
        ],
        vec![TableRow::new(
            TableRowId(1),
            vec!["设备".into(), "10".into()],
        )],
        f,
        ActionId(8),
    );
    let mut grid = DataGridView::new(table);
    grid.editable_columns.insert(TableColumnId(2));
    grid
}
fn sized_grid(grid: &DataGridView) -> View {
    let mut view = grid.build().unwrap();
    view.style.width = Dimension::Length(400.0);
    view.style.height = Dimension::Length(200.0);
    view
}
#[test]
fn datagrid_edits_cells_and_resizes_columns_without_mutating_source() {
    let (mut text, f) = text();
    let mut grid = grid(f);
    let mut tree = Tree::from_view(sized_grid(&grid), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    let result = click(&mut tree, 150.0, 55.0);
    assert!(
        result
            .actions
            .iter()
            .any(|a| a.action.payload::<DataGridEvent>()
                == Some(&DataGridEvent::Selected(TableRowId(1))))
    );
    key(&mut tree, Key::Character("a".into()), true);
    key(&mut tree, Key::Character("42".into()), false);
    layout(&mut tree, &mut text);
    let submitted = key(&mut tree, Key::Enter, false);
    assert_eq!(
        submitted.actions[0].action.payload::<DataGridEvent>(),
        Some(&DataGridEvent::CellEdited {
            cell: CellAddress {
                row: TableRowId(1),
                column: TableColumnId(2)
            },
            value: "42".into()
        })
    );
    assert_eq!(grid.table.rows[0].cells[1], "10");
    assert_eq!(
        key(&mut tree, Key::Escape, false).actions[0]
            .action
            .payload::<DataGridEvent>(),
        Some(&DataGridEvent::EditCancelled {
            cell: CellAddress {
                row: TableRowId(1),
                column: TableColumnId(2)
            }
        })
    );
    layout(&mut tree, &mut text);
    assert_eq!(
        key(&mut tree, Key::Enter, false).actions[0]
            .action
            .payload::<DataGridEvent>(),
        Some(&DataGridEvent::CellEdited {
            cell: CellAddress {
                row: TableRowId(1),
                column: TableColumnId(2)
            },
            value: "10".into()
        })
    );

    tree.dispatch_input(InputEvent::PointerMoved(Point::new(116.0, 15.0)))
        .unwrap();
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(Point::new(116.0, 15.0)),
    })
    .unwrap();
    let result = tree
        .dispatch_input(InputEvent::PointerMoved(Point::new(146.0, 15.0)))
        .unwrap();
    assert_eq!(
        result.actions[0].action.payload::<DataGridEvent>(),
        Some(&DataGridEvent::ColumnResized {
            column: TableColumnId(1),
            width: 150.0
        })
    );
    grid.table.columns[0].width = 150.0;
    tree.reconcile(sized_grid(&grid)).unwrap();
    layout(&mut tree, &mut text);
    let result = tree
        .dispatch_input(InputEvent::PointerMoved(Point::new(156.0, 15.0)))
        .unwrap();
    assert_eq!(
        result.actions[0].action.payload::<DataGridEvent>(),
        Some(&DataGridEvent::ColumnResized {
            column: TableColumnId(1),
            width: 160.0
        })
    );
}

fn verify_pixels(renderer: &mut impl zui_paint::OffscreenRenderer) {
    let (mut text, f) = text();
    let picker = ColorPickerView::themed(
        &Theme::dashboard(),
        Color::rgba(255, 0, 0, 128),
        f,
        ActionId(10),
    );
    let mut view = picker.build();
    view.style.width = Dimension::Length(300.0);
    let mut tree = Tree::from_view(view, Default::default()).unwrap();
    layout(&mut tree, &mut text);
    for scale in [1, 2] {
        let pixels = renderer
            .render(
                &tree.scene().unwrap(),
                zui_paint::Frame {
                    extent: zui_types::Extent::new(320 * scale, 300 * scale),
                    scale: scale as f64,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let offset = (5 * scale) as usize * pixels.stride() + (5 * scale * 4) as usize;
        let actual = &pixels.pixels()[offset..offset + 4];
        for (a, b) in actual.iter().zip([255u8, 127, 127, 255]) {
            assert!(a.abs_diff(b) <= 2, "{actual:?}");
        }
    }
    let separator = SeparatorView {
        axis: SeparatorAxis::Horizontal,
        color: Color::WHITE,
        thickness: 2.0,
    };
    let mut tree = Tree::from_view(
        View::new(separator).styled(zui_api::layout::LayoutStyle {
            width: Dimension::Length(40.0),
            height: Dimension::Length(10.0),
            ..Default::default()
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
                    extent: zui_types::Extent::new(40 * scale, 10 * scale),
                    scale: scale as f64,
                    background: Color::BLACK,
                },
            )
            .unwrap();
        let offset = (5 * scale) as usize * pixels.stride() + (20 * scale * 4) as usize;
        assert_eq!(&pixels.pixels()[offset..offset + 4], &[255, 255, 255, 255]);
        assert_eq!(&pixels.pixels()[..4], &[0, 0, 0, 255]);
    }
}
#[cfg(feature = "cpu")]
#[test]
fn picker_and_separator_pixels_cpu() {
    verify_pixels(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a GPU adapter"]
fn picker_and_separator_pixels_gpu() {
    verify_pixels(
        &mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap(),
    );
}

#[cfg(feature = "cpu")]
#[test]
fn grid_resize_divider_is_thin_and_highlights_without_filling_hit_target() {
    use zui_paint::{Frame, OffscreenRenderer};
    use zui_theme::ColorRole;
    use zui_types::Extent;
    let (mut text, f) = text();
    let grid = grid(f);
    let mut tree = Tree::from_view(sized_grid(&grid), Default::default()).unwrap();
    layout(&mut tree, &mut text);
    for hovered in [false, true, false] {
        tree.dispatch_input(InputEvent::PointerMoved(Point::new(
            if hovered { 116.0 } else { 350.0 },
            15.0,
        )))
        .unwrap();
        for scale in [1_u32, 2] {
            let pixels = zui_render_cpu::CpuRenderer::default()
                .render(
                    &tree.scene().unwrap(),
                    Frame {
                        extent: Extent::new(400 * scale, 200 * scale),
                        scale: f64::from(scale),
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let expected = grid.table.theme.color(if hovered {
                ColorRole::Accent
            } else {
                ColorRole::Border
            });
            for (left, active) in [(112, hovered), (232, false)] {
                let mut count = 0;
                for x in left * scale..(left + 8) * scale {
                    let i = (15 * scale) as usize * pixels.stride() + x as usize * 4;
                    let p = &pixels.pixels()[i..i + 4];
                    if p != [0, 0, 0, 255] {
                        let c = if active {
                            expected
                        } else {
                            grid.table.theme.color(ColorRole::Border)
                        };
                        assert_eq!(p, [c.r, c.g, c.b, c.a]);
                        count += 1;
                    }
                }
                assert_eq!(count, if active { 2 * scale } else { scale });
            }
        }
    }
}
