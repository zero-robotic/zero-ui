use std::collections::{BTreeMap, BTreeSet};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges},
    view::View,
};
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Rect, Size};
use zui_widgets::*;
#[derive(Clone)]
pub struct State {
    pub name: String,
    pub error: Option<String>,
    start: Date,
    end: Date,
    month: (u16, u8),
    date_open: Option<bool>,
    date_anchor: Option<Rect>,
    time: Time,
    color: Color,
    hsv: Option<HsvColor>,
    rows: Vec<TableRow>,
    widths: [f32; 2],
    sort: Option<TableSort>,
    selected: Option<TableRowId>,
    errors: BTreeMap<CellAddress, String>,
    page: u64,
    subdir: bool,
    file_selected: Option<FileEntryId>,
    filename: String,
    mode: FilePickerMode,
    status: String,
    toast: Option<(u64, String)>,
    sequence: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            name: "采集设备 A".into(),
            error: None,
            start: Date::new(2026, 11, 1).unwrap(),
            end: Date::new(2026, 11, 30).unwrap(),
            month: (2026, 11),
            date_open: None,
            date_anchor: None,
            time: Time::new(9, 30, 0).unwrap(),
            hsv: None,
            color: Color::rgb(32, 200, 220),
            rows: (1..=6)
                .map(|id| {
                    TableRow::new(
                        TableRowId(id),
                        vec![format!("参数 {id}"), format!("{}", id * 10)],
                    )
                })
                .collect(),
            widths: [150.0, 150.0],
            sort: None,
            selected: None,
            errors: BTreeMap::new(),
            page: 1,
            subdir: false,
            file_selected: None,
            filename: "settings.json".into(),
            mode: FilePickerMode::OpenFile,
            status: "就绪".into(),
            toast: None,
            sequence: 0,
        }
    }
}
pub enum Message {
    Save,
    Notify,
    Start(DateEvent),
    End(DateEvent),
    Time(Time),
    Color(Color),
    Hsv(HsvChanged),
    Name(String),
    Grid(DataGridEvent),
    File(FilePickerEvent),
    Filename(String),
    Crumb(u64),
    Page(u64),
    Dismiss,
    CloseDate,
    Mode(FilePickerMode),
}
impl State {
    pub fn map(&self, e: &ActionEvent) -> Option<Message> {
        let a = &e.action;
        match a.id {
            ActionId(300) if a.payload::<ButtonActivated>().is_some() => Some(Message::Save),
            ActionId(301) if a.payload::<ButtonActivated>().is_some() => Some(Message::Notify),
            ActionId(302) => a.payload::<DateEvent>().copied().map(Message::Start),
            ActionId(303) => a.payload::<DateEvent>().copied().map(Message::End),
            ActionId(304) => a.payload::<TimeChanged>().map(|e| Message::Time(e.time)),
            ActionId(305) => a
                .payload::<ColorChanged>()
                .map(|e| Message::Color(e.color))
                .or_else(|| a.payload::<HsvChanged>().copied().map(Message::Hsv)),
            ActionId(306) => match a.payload::<TextInputEvent>() {
                Some(TextInputEvent::Changed(s)) => Some(Message::Name(s.clone())),
                _ => None,
            },
            ActionId(307) => a.payload::<DataGridEvent>().cloned().map(Message::Grid),
            ActionId(308) => a.payload::<FilePickerEvent>().cloned().map(Message::File),
            ActionId(309) => match a.payload::<TextInputEvent>() {
                Some(TextInputEvent::Changed(s)) => Some(Message::Filename(s.clone())),
                _ => None,
            },
            ActionId(310) => a
                .payload::<BreadcrumbSelected>()
                .map(|e| Message::Crumb(e.id)),
            ActionId(311) => a.payload::<PageChanged>().map(|e| Message::Page(e.page)),
            ActionId(312) => a
                .payload::<NotificationDismissed>()
                .map(|_| Message::Dismiss),
            ActionId(313) => a.payload::<SegmentChanged>().map(|e| {
                Message::Mode(match e.selected.0 {
                    1 => FilePickerMode::SaveFile,
                    2 => FilePickerMode::SelectDirectory,
                    _ => FilePickerMode::OpenFile,
                })
            }),
            ActionId(320) => a.payload::<PopupDismissed>().map(|_| Message::CloseDate),
            _ => None,
        }
    }
    fn notify(&mut self, text: String) {
        self.sequence += 1;
        self.status = text.clone();
        self.toast = Some((self.sequence, text));
    }
    pub fn apply(&mut self, m: Message) {
        match m {
            Message::Save => {
                self.error = self
                    .name
                    .trim()
                    .is_empty()
                    .then(|| "设备名称不能为空".into());
                if self.error.is_none() {
                    self.notify(format!(
                        "表单已确认：{}，{} 至 {}",
                        self.name, self.start, self.end
                    ));
                }
            }
            Message::Notify => self.notify("操作完成，可继续编辑其他控件".into()),
            Message::Name(name) => {
                self.name = name;
                if !self.name.trim().is_empty() {
                    self.error = None;
                }
            }
            Message::Time(time) => {
                self.time = time;
                self.status = "时间已更新".into();
            }
            Message::Hsv(e) => {
                self.hsv = Some(e.hsv);
                self.color = e.color;
            }
            Message::Color(color) => {
                self.color = color;
                if self.hsv.and_then(|h| h.to_color(color.a)) != Some(color) {
                    self.hsv = None;
                }
                self.status = "颜色已更新".into();
            }
            Message::Start(event) => self.date(true, event),
            Message::End(event) => self.date(false, event),
            Message::CloseDate => self.date_open = None,
            Message::Grid(DataGridEvent::Selected(id)) => self.selected = Some(id),
            Message::Grid(DataGridEvent::EditCancelled { cell }) => {
                self.errors.remove(&cell);
            }
            Message::Grid(DataGridEvent::SortRequested(sort)) => {
                self.sort = Some(sort);
                self.page = 1;
            }
            Message::Grid(DataGridEvent::ColumnResized { column, width }) => {
                self.widths[column.0 as usize] = width
            }
            Message::Grid(DataGridEvent::CellEdited { cell, value }) => {
                if cell.column == TableColumnId(1) && value.parse::<u8>().is_ok_and(|v| v <= 100) {
                    if let Some(row) = self.rows.iter_mut().find(|r| r.id == cell.row) {
                        row.cells[1] = value;
                    }
                    self.errors.remove(&cell);
                    self.status = "参数已提交".into();
                } else {
                    self.errors.insert(cell, "请输入 0–100 的整数".into());
                }
            }
            Message::Page(page) => self.page = page,
            Message::Crumb(id) => {
                self.subdir = id != 0;
                self.file_selected = None;
            }
            Message::Filename(name) => self.filename = name,
            Message::Mode(mode) => {
                self.mode = mode;
                self.file_selected = None;
            }
            Message::File(FilePickerEvent::Parent) => {
                self.subdir = false;
                self.file_selected = None;
            }
            Message::File(FilePickerEvent::EnterDirectory(_)) => {
                self.subdir = true;
                self.file_selected = None;
            }
            Message::File(FilePickerEvent::Selected(id)) => self.file_selected = Some(id),
            Message::File(FilePickerEvent::OpenFile(id)) => {
                self.notify(format!("已选择演示文件 {}", id.0))
            }
            Message::File(FilePickerEvent::SaveFile { name }) => {
                self.notify(format!("保存请求：{name}"))
            }
            Message::File(FilePickerEvent::ChooseDirectory { directory }) => {
                self.notify(format!("已选择目录：{directory}"))
            }
            Message::Dismiss => self.toast = None,
        }
    }
    fn date(&mut self, start: bool, event: DateEvent) {
        match event {
            DateEvent::Open => {
                self.date_open = Some(start);
                let d = if start { self.start } else { self.end };
                self.month = (d.year, d.month);
            }
            DateEvent::MonthChanged { year, month } => self.month = (year, month),
            DateEvent::Selected(date) => {
                if (start && date <= self.end) || (!start && date >= self.start) {
                    if start {
                        self.start = date;
                    } else {
                        self.end = date;
                    }
                    self.date_open = None;
                    self.status = "查询日期范围已更新".into();
                }
            }
        }
    }
    fn picker(&self, theme: &Theme, f: &[String], start: bool) -> DatePickerView {
        let mut p = DatePickerView::themed(
            theme,
            if start { self.start } else { self.end },
            f.to_vec(),
            ActionId(if start { 302 } else { 303 }),
        );
        if start {
            p.calendar.max = Some(self.end);
        } else {
            p.calendar.min = Some(self.start);
        }
        p
    }
    pub fn open_date(&mut self, start: bool, anchor: Rect) {
        self.date_anchor = Some(anchor);
        self.date(start, DateEvent::Open);
    }
    pub fn overlay(&self, theme: &Theme, f: &[String]) -> Option<View> {
        let start = self.date_open?;
        let mut picker = self.picker(theme, f, start);
        picker.calendar.year = self.month.0;
        picker.calendar.month = self.month.1;
        Some(picker.popup(self.date_anchor?, ActionId(320)).unwrap())
    }

    pub fn view(&self, theme: &Theme, f: &[String]) -> View {
        let text = |s: String| View::new(TextView::themed(theme, s, f.to_vec(), TypeRole::Caption));
        let toolbar = ToolbarView::themed(theme).build([
            View::new({
                let mut button = ButtonView::themed(theme, "确认表单", f.to_vec(), ActionId(300));
                button.style = ButtonStyle::primary(theme);
                button
            }),
            View::new(SeparatorView::themed(theme, SeparatorAxis::Vertical)),
            View::new(ButtonView::themed(
                theme,
                "显示通知",
                f.to_vec(),
                ActionId(301),
            )),
        ]);
        let mut name = TextInputView::themed(theme, self.name.clone(), f.to_vec(), ActionId(306));
        name.invalid = self.error.is_some();
        let mut field = FormField::new("name", "设备名称", View::new(name));
        field.required = true;
        field.error = self.error.clone();
        field.help = Some("清空后点击“确认表单”查看必填校验".into());
        let range = DateRangePickerView {
            start: self.picker(theme, f, true),
            end: self.picker(theme, f, false),
        }
        .build()
        .unwrap();
        let time = TimePickerView::themed(theme, self.time, f.to_vec(), ActionId(304))
            .build()
            .unwrap();
        let mut form = FormView::themed(theme, f.to_vec());
        form.layout = FormLayout::Inline { label_width: 90.0 };
        let form = GroupBoxView::themed(theme, "设置表单", f.to_vec()).build(form.build(vec![
            field,
            FormField::new("dates", "查询范围", range),
            FormField::new("time", "执行时间", time),
        ]));
        let mut rows = self.rows.clone();
        if let Some(sort) = self.sort {
            rows.sort_by(|a, b| {
                if sort.column.0 == 1 {
                    a.cells[1]
                        .parse::<u8>()
                        .unwrap()
                        .cmp(&b.cells[1].parse::<u8>().unwrap())
                } else {
                    a.cells[0].cmp(&b.cells[0])
                }
            });
            if sort.direction == SortDirection::Descending {
                rows.reverse();
            }
        }
        let rows: Vec<_> = rows
            .into_iter()
            .skip((self.page as usize - 1) * 2)
            .take(2)
            .collect();
        let selected = self.selected.filter(|id| rows.iter().any(|r| r.id == *id));
        let mut table = TableView::themed(
            theme,
            vec![
                TableColumn::new(TableColumnId(0), "参数", self.widths[0]),
                TableColumn::new(TableColumnId(1), "数值（Enter 提交）", self.widths[1]),
            ],
            rows,
            f.to_vec(),
            ActionId(307),
        );
        table.selected = selected;
        table.sort = self.sort;
        let visible: BTreeSet<_> = table.rows.iter().map(|r| r.id).collect();
        let mut grid = DataGridView::new(table);
        grid.editable_columns.insert(TableColumnId(1));
        grid.cell_errors = self
            .errors
            .iter()
            .filter(|(cell, _)| visible.contains(&cell.row))
            .map(|(cell, error)| (*cell, error.clone()))
            .collect();
        let mut grid = grid.build().unwrap();
        grid.style.height = Dimension::Length(190.0);
        grid.style.width = Dimension::Percent(1.0);
        let grid = GroupBoxView::themed(theme, "DataGrid：拖动列边界；数值范围 0–100", f.to_vec())
            .build(
                column([
                    grid,
                    PaginationView::themed(theme, self.page, 3, f.to_vec(), ActionId(311))
                        .build()
                        .unwrap(),
                ])
                .style(|s| s.gap = Size::new(0.0, 8.0))
                .build(),
            );
        let mut crumbs = vec![BreadcrumbItem::new(0, "演示目录")];
        if self.subdir {
            crumbs.push(BreadcrumbItem::new(1, "配置"));
        }
        let crumbs = BreadcrumbView::themed(theme, crumbs, f.to_vec(), ActionId(310))
            .build()
            .unwrap();
        let entries = if self.subdir {
            vec![
                FileEntry::new(FileEntryId(20), "settings.json", false),
                FileEntry::new(FileEntryId(21), "device.toml", false),
            ]
        } else {
            vec![
                FileEntry::new(FileEntryId(1), "配置", true),
                FileEntry::new(FileEntryId(2), "README.txt", false),
            ]
        };
        let mut file = FilePickerView::themed(
            theme,
            if self.subdir { "/demo/config" } else { "/demo" },
            entries,
            f.to_vec(),
            ActionId(308),
        );
        file.selected = self.file_selected;
        file.filename_action = ActionId(309);
        file.filename = self.filename.clone();
        file.mode = self.mode;
        file.can_go_parent = self.subdir;
        let mode = SegmentedControlView::themed(
            theme,
            vec![
                SegmentOption::new(SegmentId(0), "打开"),
                SegmentOption::new(SegmentId(1), "保存"),
                SegmentOption::new(SegmentId(2), "目录"),
            ],
            f.to_vec(),
            Some(SegmentId(match self.mode {
                FilePickerMode::OpenFile => 0,
                FilePickerMode::SaveFile => 1,
                FilePickerMode::SelectDirectory => 2,
            })),
            ActionId(313),
        );
        let file = GroupBoxView::themed(theme, "文件选择（演示数据，不读写磁盘）", f.to_vec())
            .build(
                column([View::new(mode), crumbs, file.build().unwrap()])
                    .style(|s| s.gap = Size::new(0.0, 8.0))
                    .build(),
            );
        let mut picker = ColorPickerView::themed(theme, self.color, f.to_vec(), ActionId(305));
        picker.hsv = self.hsv;
        let color = GroupBoxView::themed(theme, "颜色选择：HSV / RGBA / 十六进制", f.to_vec())
            .build(picker.build());
        let mut content = vec![
            text("补充控件：表单 / DataGrid / 日期时间 / 导航 / 通知 / 专用选择器".into()),
            toolbar,
            form,
            View::new(SeparatorView::themed(theme, SeparatorAxis::Horizontal)),
            grid,
            file,
            color,
        ];
        if let Some((id, message)) = &self.toast {
            let mut note =
                NotificationView::themed(theme, *id, "提示", message.clone(), f.to_vec());
            note.dismiss = Some(ActionId(312));
            note.kind = NotificationKind::Success;
            content.push(ToastView::new(note).build().keyed("toast"));
        }
        content.push(StatusBarView::themed(theme).build(
            text(self.status.clone()),
            text(format!("第 {} / 3 页", self.page)),
        ));
        column(content)
            .style(|s| {
                s.gap = Size::new(0.0, 12.0);
                s.padding = Edges::all(4.0);
            })
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_actions_validate_edit_and_link_navigation() {
        let mut state = State::default();
        state.apply(Message::Name(String::new()));
        state.apply(Message::Save);
        assert!(state.error.is_some());
        state.apply(Message::Name("设备".into()));
        state.apply(Message::Save);
        assert!(state.error.is_none());
        assert!(state.toast.is_some());
        let cell = CellAddress {
            row: TableRowId(1),
            column: TableColumnId(1),
        };
        state.apply(Message::Grid(DataGridEvent::CellEdited {
            cell,
            value: "999".into(),
        }));
        assert!(state.errors.contains_key(&cell));
        assert_eq!(state.rows[0].cells[1], "10");
        state.apply(Message::Grid(DataGridEvent::CellEdited {
            cell,
            value: "42".into(),
        }));
        assert!(!state.errors.contains_key(&cell));
        assert_eq!(state.rows[0].cells[1], "42");
        state.apply(Message::File(FilePickerEvent::EnterDirectory(FileEntryId(
            1,
        ))));
        assert!(state.subdir);
        state.apply(Message::Crumb(0));
        assert!(!state.subdir);
        state.apply(Message::Page(3));
        assert_eq!(state.page, 3);
        state.apply(Message::Start(DateEvent::Open));
        assert_eq!(state.date_open, Some(true));
        state.apply(Message::Start(DateEvent::Selected(
            Date::new(2026, 11, 10).unwrap(),
        )));
        assert_eq!(state.start.day, 10);
        assert!(state.date_open.is_none());
    }
    #[test]
    fn expanded_catalog_states_layout_on_narrow_and_wide_windows() {
        let mut text = zui_text::ParleyText::default();
        let f = text
            .register_font(std::sync::Arc::from(
                include_bytes!("../../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let theme = Theme::dashboard();
        let mut state = State::default();
        for mode in [
            FilePickerMode::OpenFile,
            FilePickerMode::SaveFile,
            FilePickerMode::SelectDirectory,
        ] {
            state.mode = mode;
            state.subdir = true;
            state.file_selected = Some(FileEntryId(20));
            state.error = Some("必填字段".into());
            state.errors.insert(
                CellAddress {
                    row: TableRowId(1),
                    column: TableColumnId(1),
                },
                "数值错误".into(),
            );
            state.toast = Some((1, "测试通知".into()));
            state.open_date(true, Rect::new(120.0, 180.0, 140.0, 28.0));
            for width in [480.0, 1100.0] {
                let content = state.view(&theme, &f);
                let overlay = state.overlay(&theme, &f).unwrap();
                let root = stack([ScrollView::vertical(content).build(), overlay])
                    .style(|s| {
                        s.width = Dimension::Percent(1.0);
                        s.height = Dimension::Percent(1.0);
                    })
                    .build();
                let mut tree = zui_core::Tree::from_view(root, Default::default()).unwrap();
                tree.layout_with_text(
                    &mut zui_layout::TaffyLayout::default(),
                    &mut text,
                    Size::new(width, 800.0),
                )
                .unwrap();
                tree.scene().unwrap();
            }
        }
    }
}
