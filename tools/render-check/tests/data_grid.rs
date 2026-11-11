use std::sync::Arc;
use zui_api::{action::ActionId, event::*, view::Key as ViewKey};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, Extent, NodeId, Point, Rect, Size};
use zui_widgets::{
    DataGridEvent, DataGridView, DataGridVirtualRows, TableColumn, TableColumnId, TableRow,
    TableRowId, TableView,
};

fn find_row(tree: &Tree, id: NodeId, row: u64) -> Option<NodeId> {
    find_key(tree, id, &ViewKey::Text(format!("row-{row}")))
}

fn find_key(tree: &Tree, id: NodeId, key: &ViewKey) -> Option<NodeId> {
    if tree.key(id).unwrap() == Some(key) {
        return Some(id);
    }
    tree.children(id)
        .unwrap()
        .iter()
        .find_map(|child| find_key(tree, *child, key))
}

fn bounds(tree: &Tree, id: NodeId) -> Rect {
    let mut rect = tree.layout_box(id).unwrap().bounds;
    let mut parent = tree.parent(id).unwrap();
    while let Some(id) = parent {
        let origin = tree.layout_box(id).unwrap().bounds.origin;
        rect.origin.x += origin.x;
        rect.origin.y += origin.y;
        parent = tree.parent(id).unwrap();
    }
    rect
}

fn layout(tree: &mut Tree, text: &mut zui_text::ParleyText) {
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        text,
        Size::new(240.0, 240.0),
    )
    .unwrap();
}

fn pointer(pressed: bool, position: Point) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(position),
    }
}

fn verify(renderer: &mut impl OffscreenRenderer) {
    for theme in [Theme::desktop_dark(), Theme::desktop_light(), Theme::dark()] {
        for virtual_rows in [false, true] {
            let mut text = zui_text::ParleyText::default();
            let families = text
                .register_font(Arc::from(
                    include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
                ))
                .unwrap();
            let mut table = TableView::themed(
                &theme,
                vec![
                    TableColumn::new(TableColumnId(1), "First", 100.0),
                    TableColumn::new(TableColumnId(2), "Second", 100.0),
                ],
                (0..if virtual_rows { 10_000 } else { 2 })
                    .map(|index| {
                        TableRow::new(TableRowId(index), vec![String::new(), String::new()])
                    })
                    .collect(),
                families,
                ActionId(7),
            );
            table.selected = Some(TableRowId(0));
            let mut grid = DataGridView::new(table);
            grid.resizable = false;
            if virtual_rows {
                grid.virtual_rows = Some(DataGridVirtualRows {
                    estimated_height: 40.0,
                    viewport_height: 120.0,
                    revision: 0,
                });
            }
            let mut tree = Tree::from_view(grid.build().unwrap(), Default::default()).unwrap();
            layout(&mut tree, &mut text);
            let assert_flat = |tree: &mut Tree,
                               renderer: &mut dyn OffscreenRenderer,
                               selected: u64| {
                let scene = tree.scene().unwrap();
                for scale in [1.0, 2.0] {
                    let pixels = renderer
                        .render(
                            &scene,
                            Frame {
                                extent: Extent::new((240.0 * scale) as u32, (240.0 * scale) as u32),
                                scale,
                                background: Color::BLACK,
                            },
                        )
                        .unwrap();
                    for row in 0..2 {
                        let rect = bounds(tree, find_row(tree, tree.root(), row).unwrap());
                        let overlay_width = if virtual_rows {
                            zui_widgets::ScrollbarStyle::default().width
                        } else {
                            0.0
                        };
                        assert_eq!(rect.size.width, 200.0);
                        let expected = theme.color(if row == selected {
                            ColorRole::SurfaceRaised
                        } else {
                            ColorRole::Surface
                        });
                        for pixel_y in ((rect.origin.y + 1.0) * scale) as usize
                            ..((rect.origin.y + rect.size.height - 1.0) * scale) as usize
                        {
                            for pixel_x in ((rect.origin.x + 1.0) * scale) as usize
                                ..((rect.origin.x + rect.size.width - overlay_width - 1.0) * scale)
                                    as usize
                            {
                                let offset = pixel_y * pixels.stride() + pixel_x * 4;
                                assert_eq!(
                                    &pixels.pixels()[offset..offset + 4],
                                    &[expected.r, expected.g, expected.b, expected.a],
                                    "row={row}, virtual={virtual_rows}, scale={scale}, pixel=({pixel_x},{pixel_y})"
                                );
                            }
                        }
                    }
                }
            };
            assert_flat(&mut tree, renderer, 0);
            let rect = bounds(&tree, find_row(&tree, tree.root(), 1).unwrap());
            let point = Point::new(rect.origin.x + 20.0, rect.origin.y + 16.0);
            tree.dispatch_input(InputEvent::PointerMoved(point))
                .unwrap();
            assert_flat(&mut tree, renderer, 0);
            tree.dispatch_input(pointer(true, point)).unwrap();
            assert_flat(&mut tree, renderer, 0);
            let result = tree.dispatch_input(pointer(false, point)).unwrap();
            assert!(
                result
                    .actions
                    .iter()
                    .any(|action| action.action.payload::<DataGridEvent>()
                        == Some(&DataGridEvent::Selected(TableRowId(1))))
            );
            assert_flat(&mut tree, renderer, 0);
            grid.table.selected = Some(TableRowId(1));
            tree.reconcile(grid.build().unwrap()).unwrap();
            layout(&mut tree, &mut text);
            assert_flat(&mut tree, renderer, 1);
            grid.table.rows[1].disabled = true;
            tree.reconcile(grid.build().unwrap()).unwrap();
            layout(&mut tree, &mut text);
            tree.dispatch_input(pointer(true, point)).unwrap();
            assert!(
                tree.dispatch_input(pointer(false, point))
                    .unwrap()
                    .actions
                    .is_empty()
            );
            assert_flat(&mut tree, renderer, 1);
            grid.table.rows[1].disabled = false;
            tree.reconcile(grid.build().unwrap()).unwrap();
            layout(&mut tree, &mut text);
            tree.dispatch_input(pointer(true, point)).unwrap();
            tree.dispatch_input(pointer(false, point)).unwrap();
            let result = tree
                .dispatch_input(InputEvent::Key(KeyInput {
                    key: Key::Enter,
                    pressed: true,
                    repeat: false,
                    text: None,
                    modifiers: Default::default(),
                }))
                .unwrap();
            assert!(
                result
                    .actions
                    .iter()
                    .any(|action| action.action.payload::<DataGridEvent>()
                        == Some(&DataGridEvent::Selected(TableRowId(1))))
            );
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn readonly_cells_keep_one_flat_row_background_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter"]
fn readonly_cells_keep_one_flat_row_background_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

#[test]
fn sorting_headers_never_changes_wrapping_or_row_positions() {
    use zui_paint::Command;
    use zui_widgets::{SortDirection, TableEvent, TableSort};
    for data_grid in [false, true] {
        for width in [100.0, 150.0, 200.0] {
            let mut text = zui_text::ParleyText::default();
            let families = text
                .register_font(Arc::from(
                    include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
                ))
                .unwrap();
            let mut table = TableView::themed(
                &Theme::desktop_dark(),
                vec![
                    TableColumn::new(TableColumnId(0), "参数", 100.0),
                    TableColumn::new(TableColumnId(1), "数值（Enter 提交）", width),
                ],
                vec![TableRow::new(
                    TableRowId(0),
                    vec!["参数 1".into(), "10".into()],
                )],
                families,
                ActionId(1),
            );
            let build = |table: &TableView| {
                if data_grid {
                    DataGridView::new(table.clone()).build().unwrap()
                } else {
                    table.build().unwrap()
                }
            };
            let mut tree = Tree::from_view(build(&table), Default::default()).unwrap();
            layout(&mut tree, &mut text);
            fn snapshot(
                tree: &Tree,
                id: NodeId,
                output: &mut Vec<(NodeId, zui_api::layout::LayoutBox)>,
            ) {
                output.push((id, tree.layout_box(id).unwrap()));
                for child in tree.children(id).unwrap() {
                    snapshot(tree, *child, output);
                }
            }
            let mut original = Vec::new();
            snapshot(&tree, tree.root(), &mut original);
            let glyphs = |scene: zui_paint::Scene| {
                scene
                    .commands()
                    .iter()
                    .filter(|command| matches!(command, Command::Glyphs { .. }))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let original_text = glyphs(tree.scene().unwrap());
            for (column, direction) in [
                (0, SortDirection::Ascending),
                (1, SortDirection::Ascending),
                (1, SortDirection::Descending),
                (0, SortDirection::Ascending),
            ] {
                let header = find_key(&tree, tree.root(), &ViewKey::Text("header".into())).unwrap();
                let target = tree.children(header).unwrap()[column as usize];
                let rect = bounds(&tree, target);
                let point = Point::new(rect.origin.x + 20.0, rect.origin.y + 10.0);
                tree.dispatch_input(pointer(true, point)).unwrap();
                let response = tree.dispatch_input(pointer(false, point)).unwrap();
                let requested = TableSort {
                    column: TableColumnId(column),
                    direction,
                };
                assert!(response.actions.iter().any(|action| if data_grid {
                    action.action.payload::<DataGridEvent>()
                        == Some(&DataGridEvent::SortRequested(requested))
                } else {
                    action.action.payload::<TableEvent>()
                        == Some(&TableEvent::SortRequested(requested))
                }));
                table.sort = Some(requested);
                tree.reconcile(build(&table)).unwrap();
                layout(&mut tree, &mut text);
                let mut current = Vec::new();
                snapshot(&tree, tree.root(), &mut current);
                assert_eq!(current, original, "grid={data_grid}, width={width}");
                assert_eq!(
                    glyphs(tree.scene().unwrap()),
                    original_text,
                    "grid={data_grid}, width={width}"
                );
            }
        }
    }
}
