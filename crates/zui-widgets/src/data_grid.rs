//! Editable text cells with controlled data, validation and column resize requests.
use crate::{
    ButtonView, Container, Icon, IconKind, ScrollView, SortDirection, TableColumnId, TableError,
    TableRowId, TableSort, TableView, TextInputView, TextView, action_button::ActionButtonView,
    column, commit_input::CommitInputView, row,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    string::String,
    vec::Vec,
};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Dimension, Edges, LayoutError, LayoutKind, LayoutStyle},
    text::TextAlign,
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::{ColorRole, TypeRole};
use zui_types::{Color, Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellAddress {
    pub row: TableRowId,
    pub column: TableColumnId,
}
#[derive(Clone, Debug, PartialEq)]
pub enum DataGridEvent {
    Selected(TableRowId),
    EditCancelled { cell: CellAddress },
    SortRequested(TableSort),
    ColumnResized { column: TableColumnId, width: f32 },
    CellEdited { cell: CellAddress, value: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataGridError {
    Table(TableError),
    UnknownColumn,
    UnknownCell,
    InvalidViewport,
}
impl core::fmt::Display for DataGridError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "data grid error: {self:?}")
    }
}
impl core::error::Error for DataGridError {}
/// Optional application-owned renderer/editor for a cell. It must emit its own actions.
pub type DataGridCellFactory = Rc<dyn Fn(CellAddress, &str, bool) -> View>;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataGridVirtualRows {
    pub estimated_height: f64,
    pub viewport_height: f32,
    /// Increment when data, order, fonts or row error content changes.
    pub revision: u64,
}
#[derive(Clone)]
pub struct DataGridView {
    pub table: TableView,
    pub editable_columns: BTreeSet<TableColumnId>,
    pub cell_errors: BTreeMap<CellAddress, String>,
    pub edit_revision: u64,
    pub resizable: bool,
    pub virtual_rows: Option<DataGridVirtualRows>,
    pub freeze_header: bool,
    pub cell_factory: Option<DataGridCellFactory>,
    pub drafts: BTreeMap<CellAddress, crate::TextInputDraft>,
}
impl DataGridView {
    pub fn new(table: TableView) -> Self {
        Self {
            table,
            editable_columns: BTreeSet::new(),
            cell_errors: BTreeMap::new(),
            edit_revision: 0,
            resizable: true,
            virtual_rows: None,
            freeze_header: false,
            cell_factory: None,
            drafts: BTreeMap::new(),
        }
    }
    pub fn build(&self) -> Result<View, DataGridError> {
        self.table.validate().map_err(DataGridError::Table)?;
        let t = &self.table;
        if self.resizable && t.columns.iter().any(|c| c.width > 4096.0) {
            return Err(DataGridError::Table(TableError::InvalidColumns));
        }
        let columns: BTreeSet<_> = t.columns.iter().map(|c| c.id).collect();
        if self.editable_columns.iter().any(|id| !columns.contains(id)) {
            return Err(DataGridError::UnknownColumn);
        }
        if self
            .cell_errors
            .keys()
            .any(|c| !columns.contains(&c.column) || !t.rows.iter().any(|r| r.id == c.row))
        {
            return Err(DataGridError::UnknownCell);
        }
        if self.virtual_rows.is_some_and(|v| {
            !v.estimated_height.is_finite()
                || v.estimated_height <= 0.0
                || !v.viewport_height.is_finite()
                || v.viewport_height <= 0.0
        }) {
            return Err(DataGridError::InvalidViewport);
        }
        let width: f32 = t.columns.iter().map(|c| c.width).sum();
        let headers = t.columns.iter().map(|c| {
            let current = t.sort.filter(|s| s.column == c.id);
            let direction = if current.is_some_and(|s| s.direction == SortDirection::Ascending) {
                SortDirection::Descending
            } else {
                SortDirection::Ascending
            };
            let mut button =
                ButtonView::themed(&t.theme, c.title.clone(), t.families.clone(), t.action);
            button.disabled = t.disabled || !c.sortable;
            button.trailing_icon = current.map(|s| {
                Icon::builtin(if s.direction == SortDirection::Ascending {
                    IconKind::ChevronUp
                } else {
                    IconKind::ChevronDown
                })
            });
            let mut children = alloc::vec![
                View::new(ActionButtonView {
                    button,
                    payload: DataGridEvent::SortRequested(TableSort {
                        column: c.id,
                        direction
                    })
                })
                .styled(LayoutStyle {
                    width: Dimension::Length(c.width - if self.resizable { 8.0 } else { 0.0 }),
                    min_width: Dimension::Length(0.0),
                    shrink: 0.0,
                    padding: Edges::all(4.0),
                    ..Default::default()
                })
            ];
            if self.resizable {
                children.push(
                    View::new(ColumnResizeView {
                        column: c.id,
                        width: c.width,
                        disabled: t.disabled,
                        action: t.action,
                        color: t.theme.color(ColorRole::Border),
                        active: t.theme.color(ColorRole::Accent),
                    })
                    .styled(LayoutStyle {
                        width: Dimension::Length(8.0),
                        min_height: Dimension::Length(36.0),
                        shrink: 0.0,
                        ..Default::default()
                    }),
                );
            }
            row(children)
                .keyed(c.id.0)
                .style(|s| {
                    s.width = Dimension::Length(c.width);
                    s.shrink = 0.0;
                })
                .build()
        });
        let mut rows = alloc::vec![row(headers).keyed("header").build()];
        if let Some(v) = self.virtual_rows {
            let data = Rc::new(self.clone());
            let body = crate::VirtualListView::measured(
                t.rows.len(),
                v.estimated_height,
                v.revision,
                move |index| data.build_row(&data.table.rows[index]),
            )
            .style(|s| {
                s.height = Dimension::Length(v.viewport_height);
                s.width = Dimension::Length(width);
            })
            .disabled(t.disabled)
            .build();
            rows.push(body);
            return Ok(ScrollView::horizontal(
                column(rows)
                    .style(|s| {
                        s.width = Dimension::Length(width);
                        s.min_width = Dimension::Length(width);
                    })
                    .build(),
            )
            .build());
        }
        if self.freeze_header {
            rows.push(
                ScrollView::vertical(column(t.rows.iter().map(|r| self.build_row(r))).build())
                    .style(|s| {
                        s.grow = 1.0;
                        s.min_height = Dimension::Length(0.0);
                    })
                    .build(),
            );
            return Ok(ScrollView::horizontal(
                column(rows)
                    .style(|s| {
                        s.width = Dimension::Length(width);
                        s.height = Dimension::Percent(1.0);
                    })
                    .build(),
            )
            .build());
        }
        rows.extend(t.rows.iter().map(|r| self.build_row(r)));
        Ok(ScrollView::both(
            column(rows)
                .style(|s| {
                    s.width = Dimension::Length(width);
                    s.min_width = Dimension::Length(width);
                })
                .build(),
        )
        .build())
    }
    fn build_row(&self, record: &crate::TableRow) -> View {
        let t = &self.table;

        let mut cells = Vec::new();
        for (index, c) in t.columns.iter().enumerate() {
            let cell = CellAddress {
                row: record.id,
                column: c.id,
            };
            let disabled = t.disabled || record.disabled;
            let selected = t.selected == Some(record.id);
            let background = t.theme.color(if selected {
                ColorRole::SurfaceRaised
            } else {
                ColorRole::Surface
            });
            let content = if let Some(factory) = &self.cell_factory {
                factory(cell, &record.cells[index], disabled)
            } else if self.editable_columns.contains(&c.id) {
                let mut input = TextInputView::themed(
                    &t.theme,
                    record.cells[index].clone(),
                    t.families.clone(),
                    t.action,
                );
                input.disabled = disabled;
                input.draft = self.drafts.get(&cell).cloned();
                input.invalid = self.cell_errors.contains_key(&cell);
                input.revision = self.edit_revision;
                input.style.background = background;
                let editor = View::new(CommitInputView {
                    cancel: Some(DataGridEvent::EditCancelled { cell }),
                    reset_on_blur: false,
                    input,
                    parse: Rc::new(move |value| {
                        Some(DataGridEvent::CellEdited {
                            cell,
                            value: value.into(),
                        })
                    }),
                })
                .keyed("input")
                .styled(LayoutStyle {
                    width: Dimension::Percent(1.0),
                    min_width: Dimension::Length(0.0),
                    min_height: Dimension::Length(28.0),
                    ..Default::default()
                });
                let mut fields = alloc::vec![editor];
                if let Some(error) = self.cell_errors.get(&cell) {
                    let mut label = TextView::themed(
                        &t.theme,
                        error.clone(),
                        t.families.clone(),
                        TypeRole::Caption,
                    );
                    label.color = t.theme.color(ColorRole::Danger);
                    fields.push(View::new(label).keyed("error"));
                }
                View::new(CellRegion {
                    row: record.id,
                    action: t.action,
                    disabled,
                })
                .styled(LayoutStyle {
                    kind: LayoutKind::Column,
                    ..Default::default()
                })
                .with_children(fields)
            } else {
                let mut button = ButtonView::themed(
                    &t.theme,
                    record.cells[index].clone(),
                    t.families.clone(),
                    t.action,
                );
                button.disabled = disabled;
                button.text_style.align = TextAlign::Start;
                button.style.normal.background = background;
                button.style.normal.foreground = t.theme.color(ColorRole::Text);
                View::new(ActionButtonView {
                    button,
                    payload: DataGridEvent::Selected(record.id),
                })
                .styled(LayoutStyle {
                    width: Dimension::Percent(1.0),
                    min_height: Dimension::Length(28.0),
                    ..Default::default()
                })
            };
            cells.push(
                Container::new(content)
                    .keyed(c.id.0)
                    .background(background)
                    .style(|s| {
                        s.width = Dimension::Length(c.width);
                        s.min_width = Dimension::Length(0.0);
                        s.shrink = 0.0;
                        s.padding = Edges::all(6.0);
                    })
                    .build(),
            );
        }
        row(cells)
            .keyed(alloc::format!("row-{}", record.id.0))
            .build()
    }
}
#[derive(Clone, Copy, PartialEq)]
struct CellRegion {
    row: TableRowId,
    action: ActionId,
    disabled: bool,
}
struct Region(CellRegion);
impl ViewSpec for CellRegion {
    type Widget = Region;
    fn build(&self) -> Region {
        Region(*self)
    }
    fn update(&self, w: &mut Region) -> Invalidation {
        w.0 = *self;
        Invalidation::None
    }
}
impl Widget for Region {
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn hit_test(&self, _: zui_types::Point, _: Size) -> bool {
        false
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        if !self.0.disabled
            && c.phase == EventPhase::Capture
            && matches!(
                e,
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                })
            )
        {
            EventResponse {
                actions: alloc::vec![WidgetAction::new(
                    self.0.action,
                    DataGridEvent::Selected(self.0.row)
                )],
                ..Default::default()
            }
        } else {
            EventResponse::default()
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
struct ColumnResizeView {
    column: TableColumnId,
    width: f32,
    disabled: bool,
    action: ActionId,
    color: Color,
    active: Color,
}
struct ColumnResize {
    properties: ColumnResizeView,
    drag: Option<(f64, f32)>,
    focused: bool,
    hovered: bool,
}
impl ViewSpec for ColumnResizeView {
    type Widget = ColumnResize;
    fn build(&self) -> ColumnResize {
        ColumnResize {
            properties: *self,
            drag: None,
            focused: false,
            hovered: false,
        }
    }
    fn update(&self, w: &mut ColumnResize) -> Invalidation {
        if self.disabled || self.column != w.properties.column || self.action != w.properties.action
        {
            w.drag = None;
        }
        let changed = *self != w.properties;
        w.properties = *self;
        if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}
impl Widget for ColumnResize {
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        if self.properties.width.is_finite() && self.properties.width >= 32.0 {
            Ok(())
        } else {
            Err(LayoutError::InvalidStyle(c.id))
        }
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        // Keep the generous hit target, but only paint a narrow, inset divider.
        let active = self.enabled() && (self.hovered || self.focused || self.drag.is_some());
        let width = if active { 2.0_f64 } else { 1.0_f64 }.min(c.size.width);
        let inset = 8.0_f64.min(c.size.height / 4.0);
        p.fill_rect(
            Rect::new(
                ((c.size.width - width) / 2.0).max(0.0) as u64 as f64,
                inset,
                width,
                (c.size.height - 2.0 * inset).max(0.0),
            ),
            if active {
                self.properties.active
            } else {
                self.properties.color
            },
        );
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        let mut r = EventResponse::default();
        match e {
            WidgetEvent::Input(InputEvent::PointerEntered) => {
                self.hovered = true;
                r.invalidation = Invalidation::Paint;
            }
            WidgetEvent::Input(InputEvent::PointerLeft) => {
                self.hovered = false;
                r.invalidation = Invalidation::Paint;
            }
            WidgetEvent::FocusChanged(value) => {
                self.focused = *value;
                if !value {
                    self.drag = None;
                }
                r.invalidation = Invalidation::Paint;
            }
            WidgetEvent::PointerCaptureLost => {
                self.drag = None;
                r.invalidation = Invalidation::Paint;
            }
            _ => {}
        }
        if !self.enabled() || c.phase != EventPhase::Target {
            return r;
        }
        let mut value = None;
        match e {
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                ..
            }) => {
                if let Some(p) = c.local_position {
                    self.drag = Some((p.x, self.properties.width));
                    r.capture = CaptureRequest::Capture;
                    r.focus = FocusRequest::Focus;
                    r.handled = true;
                    r.invalidation = Invalidation::Paint;
                }
            }
            WidgetEvent::Input(InputEvent::PointerMoved(_)) => {
                if let (Some((start, width)), Some(p)) = (self.drag, c.local_position) {
                    value =
                        Some(width as f64 + p.x - start + (self.properties.width - width) as f64);
                }
            }
            WidgetEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: false,
                ..
            }) => {
                if self.drag.take().is_some() {
                    r.capture = CaptureRequest::Release;
                    r.handled = true;
                    r.invalidation = Invalidation::Paint;
                }
            }
            WidgetEvent::Input(InputEvent::Key(k))
                if self.focused
                    && k.pressed
                    && !k.modifiers.control
                    && !k.modifiers.alt
                    && !k.modifiers.super_key =>
            {
                value = match k.key {
                    Key::ArrowLeft => Some(
                        self.properties.width as f64 - if k.modifiers.shift { 32.0 } else { 8.0 },
                    ),
                    Key::ArrowRight => Some(
                        self.properties.width as f64 + if k.modifiers.shift { 32.0 } else { 8.0 },
                    ),
                    Key::Home => Some(32.0),
                    Key::End => Some(4096.0),
                    Key::Escape => {
                        self.drag = None;
                        r.capture = CaptureRequest::Release;
                        r.handled = true;
                        None
                    }
                    _ => None,
                };
            }
            _ => {}
        }
        if let Some(value) = value.filter(|v| v.is_finite()) {
            let width = value.clamp(32.0, 4096.0) as f32;
            r.handled = true;
            if width != self.properties.width {
                r.actions.push(WidgetAction::new(
                    self.properties.action,
                    DataGridEvent::ColumnResized {
                        column: self.properties.column,
                        width,
                    },
                ));
            }
        }
        r
    }
}
