//! Read-only cell table with controlled row selection and sort requests.
use crate::{
    Button, ButtonActivated, ButtonView, Icon, IconKind, ScrollView, Text, TextView,
    action_button::ActionButtonView, column, row,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Dimension, LayoutBox, LayoutError, LayoutStyle},
    text::{TextAlign, TextStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Rect, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TableRowId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TableColumnId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableSort {
    pub column: TableColumnId,
    pub direction: SortDirection,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableEvent {
    Selected(TableRowId),
    SortRequested(TableSort),
}
#[derive(Clone, Debug, PartialEq)]
pub struct TableColumn {
    pub id: TableColumnId,
    pub title: String,
    pub width: f32,
    pub sortable: bool,
}
impl TableColumn {
    pub fn new(id: TableColumnId, title: impl Into<String>, width: f32) -> Self {
        Self {
            id,
            title: title.into(),
            width,
            sortable: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TableRow {
    pub id: TableRowId,
    pub cells: Vec<String>,
    pub disabled: bool,
}
impl TableRow {
    pub fn new(id: TableRowId, cells: Vec<String>) -> Self {
        Self {
            id,
            cells,
            disabled: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableError {
    InvalidColumns,
    DuplicateRow(TableRowId),
    CellCount(TableRowId),
    UnknownSelection,
    InvalidSort,
}
impl core::fmt::Display for TableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "table error: {self:?}")
    }
}
impl core::error::Error for TableError {}
#[derive(Clone)]
pub struct TableView {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub selected: Option<TableRowId>,
    pub sort: Option<TableSort>,
    pub action: ActionId,
    pub disabled: bool,
    pub theme: Theme,
    pub families: Vec<String>,
}
impl TableView {
    pub fn themed(
        theme: &Theme,
        columns: Vec<TableColumn>,
        rows: Vec<TableRow>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            columns,
            rows,
            selected: None,
            sort: None,
            action,
            disabled: false,
            theme: theme.clone(),
            families,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), TableError> {
        let mut ids = BTreeSet::new();
        let width: f32 = self.columns.iter().map(|c| c.width).sum();
        if self.columns.is_empty()
            || !width.is_finite()
            || self
                .columns
                .iter()
                .any(|c| !ids.insert(c.id) || !c.width.is_finite() || c.width < 32.0)
        {
            return Err(TableError::InvalidColumns);
        }
        let mut ids = BTreeSet::new();
        for r in &self.rows {
            if !ids.insert(r.id) {
                return Err(TableError::DuplicateRow(r.id));
            }
            if r.cells.len() != self.columns.len() {
                return Err(TableError::CellCount(r.id));
            }
        }
        if self.selected.is_some_and(|id| !ids.contains(&id)) {
            return Err(TableError::UnknownSelection);
        }
        if self
            .sort
            .is_some_and(|s| !self.columns.iter().any(|c| c.id == s.column && c.sortable))
        {
            return Err(TableError::InvalidSort);
        }
        Ok(())
    }
    pub fn build(&self) -> Result<View, TableError> {
        self.validate()?;
        let width: f32 = self.columns.iter().map(|c| c.width).sum();
        let headers = self.columns.iter().map(|c| {
            let mut button = ButtonView::themed(
                &self.theme,
                c.title.clone(),
                self.families.clone(),
                self.action,
            );
            button.disabled = self.disabled || !c.sortable;
            let current = self.sort.filter(|s| s.column == c.id);
            button.trailing_icon = current.map(|s| {
                Icon::builtin(if s.direction == SortDirection::Ascending {
                    IconKind::ChevronUp
                } else {
                    IconKind::ChevronDown
                })
            });
            let direction = if current.is_some_and(|s| s.direction == SortDirection::Ascending) {
                SortDirection::Descending
            } else {
                SortDirection::Ascending
            };
            View::new(ActionButtonView {
                button,
                payload: TableEvent::SortRequested(TableSort {
                    column: c.id,
                    direction,
                }),
            })
            .keyed(c.id.0)
            .styled(LayoutStyle {
                width: Dimension::Length(c.width),
                min_height: Dimension::Length(36.0),
                shrink: 0.0,
                ..Default::default()
            })
        });
        let mut children = alloc::vec![row(headers).keyed("header").build()];
        let typography = self.theme.typography(TypeRole::Body);
        for data in &self.rows {
            let mut button =
                ButtonView::themed(&self.theme, "", self.families.clone(), self.action);
            button.disabled = self.disabled || data.disabled;
            let background = if self.selected == Some(data.id) {
                self.theme.color(ColorRole::SurfaceRaised)
            } else {
                self.theme.color(ColorRole::Surface)
            };
            button.style.normal.background = background;
            button.style.normal.foreground = self.theme.color(ColorRole::Text);
            button.style.hovered.background = self.theme.color(ColorRole::SurfaceRaised);
            button.style.hovered.foreground = self.theme.color(ColorRole::Text);
            button.style.pressed = button.style.hovered;
            let style = TextStyle {
                families: self.families.clone(),
                font_size: typography.font_size(),
                line_height: typography.line_height(),
                align: TextAlign::Start,
            };
            children.push(
                View::new(RowView {
                    button,
                    id: data.id,
                    cells: data.cells.clone(),
                    widths: self.columns.iter().map(|c| c.width).collect(),
                    style,
                })
                .keyed(alloc::format!("row-{}", data.id.0))
                .styled(LayoutStyle {
                    width: Dimension::Length(width),
                    shrink: 0.0,
                    ..Default::default()
                }),
            );
        }
        Ok(ScrollView::both(
            column(children)
                .style(|s| {
                    s.width = Dimension::Length(width);
                    s.min_width = Dimension::Length(width);
                })
                .build(),
        )
        .build())
    }
}
#[derive(Clone, PartialEq)]
struct RowView {
    button: ButtonView,
    id: TableRowId,
    cells: Vec<String>,
    widths: Vec<f32>,
    style: TextStyle,
}
impl RowView {
    fn texts(&self) -> Vec<Text> {
        self.cells
            .iter()
            .map(|c| {
                TextView::new(
                    c.clone(),
                    self.style.clone(),
                    self.button.style.normal.foreground,
                )
                .build()
            })
            .collect()
    }
}
struct RowWidget {
    properties: RowView,
    button: Button,
    texts: Vec<Text>,
}
impl ViewSpec for RowView {
    type Widget = RowWidget;
    fn build(&self) -> RowWidget {
        RowWidget {
            properties: self.clone(),
            button: self.button.build(),
            texts: self.texts(),
        }
    }
    fn update(&self, w: &mut RowWidget) -> Invalidation {
        let mut result = self.button.update(&mut w.button);
        if self.cells != w.properties.cells
            || self.style != w.properties.style
            || self.widths != w.properties.widths
        {
            w.texts = self.texts();
            result = Invalidation::Layout;
        }
        if self.id != w.properties.id {
            w.button.cancel_press();
        }
        w.properties = self.clone();
        result
    }
}
impl Widget for RowWidget {
    fn enabled(&self) -> bool {
        self.button.enabled()
    }
    fn focusable(&self) -> bool {
        self.button.focusable()
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        let mut height: f64 = 36.0;
        for (text, width) in self.texts.iter_mut().zip(&self.properties.widths) {
            let mut input = c.input;
            input.known_width = Some(width - 16.0);
            input.known_height = None;
            let size = text.measure(MeasureContext {
                id: c.id,
                input,
                text: c.text,
            })?;
            height = height.max(size.height + 12.0);
        }
        Ok(Size::new(
            self.properties.widths.iter().map(|v| *v as f64).sum(),
            height,
        ))
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.button.prepare(PrepareContext {
            id: c.id,
            layout: c.layout,
            content_size: c.content_size,
            text: c.text,
        })?;
        for (text, width) in self.texts.iter_mut().zip(&self.properties.widths) {
            let size = Size::new((*width - 16.0) as f64, c.content_size.height);
            text.prepare(PrepareContext {
                id: c.id,
                layout: LayoutBox::new(Rect::new(0.0, 0.0, size.width, size.height)),
                content_size: size,
                text: c.text,
            })?;
        }
        Ok(())
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.button.paint(c, p);
        let color = if self.properties.button.disabled {
            self.properties.button.style.disabled.foreground
        } else {
            self.properties.button.style.normal.foreground
        };
        let mut x = c.content_bounds.origin.x;
        for (text, width) in self.texts.iter().zip(&self.properties.widths) {
            let rect = Rect::new(
                x + 8.0,
                c.content_bounds.origin.y + 6.0,
                (*width - 16.0) as f64,
                (c.content_bounds.size.height - 12.0).max(0.0),
            );
            p.with_clip(Path::rectangle(rect), |p| {
                text.paint_color(
                    PaintContext {
                        content_bounds: rect,
                        ..c
                    },
                    p,
                    color,
                )
            });
            x += *width as f64;
        }
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        let mut r = self.button.event(c, e);
        for a in &mut r.actions {
            if a.payload::<ButtonActivated>().is_some() {
                *a = WidgetAction::new(a.id, TableEvent::Selected(self.properties.id));
            }
        }
        r
    }
}
