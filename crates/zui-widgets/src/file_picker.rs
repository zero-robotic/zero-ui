//! Backend-neutral file selection. Applications enumerate directories and perform I/O.
use crate::{
    ButtonView, Icon, IconKind, ListBox, ListBoxView, ListItem, ListItemId, ListSelectionChanged,
    TextInputView, TextView, action_button::ActionButtonView, column, row,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::{ActionId, WidgetAction},
    event::*,
    layout::{Dimension, LayoutError, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_paint::Painter;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileEntryId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub id: FileEntryId,
    pub name: String,
    pub directory: bool,
    pub disabled: bool,
}
impl FileEntry {
    pub fn new(id: FileEntryId, name: impl Into<String>, directory: bool) -> Self {
        Self {
            id,
            name: name.into(),
            directory,
            disabled: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilePickerMode {
    OpenFile,
    SaveFile,
    SelectDirectory,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilePickerEvent {
    Parent,
    Selected(FileEntryId),
    EnterDirectory(FileEntryId),
    OpenFile(FileEntryId),
    SaveFile { name: String },
    ChooseDirectory { directory: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilePickerError {
    DuplicateId,
    UnknownSelection,
    InvalidEntry,
}
impl core::fmt::Display for FilePickerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "file picker error: {self:?}")
    }
}
impl core::error::Error for FilePickerError {}
pub struct FilePickerView {
    pub directory: String,
    pub entries: Vec<FileEntry>,
    pub selected: Option<FileEntryId>,
    pub mode: FilePickerMode,
    pub filename: String,
    pub extensions: Vec<String>,
    pub show_hidden: bool,
    pub can_go_parent: bool,
    pub disabled: bool,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
    pub filename_action: ActionId,
}
impl FilePickerView {
    pub fn themed(
        theme: &Theme,
        directory: impl Into<String>,
        entries: Vec<FileEntry>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            directory: directory.into(),
            entries,
            selected: None,
            mode: FilePickerMode::OpenFile,
            filename: String::new(),
            extensions: Vec::new(),
            show_hidden: false,
            can_go_parent: true,
            disabled: false,
            theme: theme.clone(),
            families,
            action,
            filename_action: action,
        }
    }
    fn visible(&self, e: &FileEntry) -> bool {
        (self.show_hidden || !e.name.starts_with('.'))
            && (e.directory
                || self.extensions.is_empty()
                || e.name.rsplit_once('.').is_some_and(|(_, ext)| {
                    self.extensions
                        .iter()
                        .any(|filter| filter.trim_start_matches('.').eq_ignore_ascii_case(ext))
                }))
    }
    pub fn valid_filename(name: &str) -> bool {
        !name.trim().is_empty()
            && name != "."
            && name != ".."
            && !name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
    }
    pub fn build(&self) -> Result<View, FilePickerError> {
        let mut ids = BTreeSet::new();
        for e in &self.entries {
            if !ids.insert(e.id) {
                return Err(FilePickerError::DuplicateId);
            }
            if !Self::valid_filename(&e.name) {
                return Err(FilePickerError::InvalidEntry);
            }
        }
        if self.selected.is_some_and(|id| !ids.contains(&id)) {
            return Err(FilePickerError::UnknownSelection);
        }
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter(|e| self.visible(e))
            .cloned()
            .collect();
        let selected = self
            .selected
            .filter(|id| entries.iter().any(|e| e.id == *id));
        let mut list = ListBoxView::themed(
            &self.theme,
            entries
                .iter()
                .map(|e| {
                    let mut item = ListItem::new(ListItemId(e.id.0), e.name.clone());
                    item.disabled = e.disabled
                        || (self.mode == FilePickerMode::SelectDirectory && !e.directory);
                    item.leading_icon = e.directory.then(|| Icon::builtin(IconKind::ChevronRight));
                    item
                })
                .collect(),
            self.families.clone(),
            selected.map(|id| ListItemId(id.0)),
            self.action,
        );
        list.disabled = self.disabled;
        let mut parent =
            ButtonView::themed(&self.theme, "上级目录", self.families.clone(), self.action);
        parent.disabled = self.disabled || !self.can_go_parent;
        let chosen = entries
            .iter()
            .find(|e| Some(e.id) == selected && !e.disabled);
        let primary = chosen.map(|e| {
            if e.directory {
                FilePickerEvent::EnterDirectory(e.id)
            } else {
                FilePickerEvent::OpenFile(e.id)
            }
        });
        let mut views = alloc::vec![
            row([
                View::new(ActionButtonView {
                    button: parent,
                    payload: FilePickerEvent::Parent
                }),
                View::new(TextView::themed(
                    &self.theme,
                    self.directory.clone(),
                    self.families.clone(),
                    TypeRole::Caption
                ))
            ])
            .style(|s| {
                s.wrap = true;
                s.gap = Size::new(8.0, 4.0);
            })
            .build(),
            View::new(FileListView {
                list,
                entries: entries.clone(),
                allow_files: self.mode == FilePickerMode::OpenFile
            })
            .styled(LayoutStyle {
                height: Dimension::Length(180.0),
                ..Default::default()
            })
        ];
        let payload = match self.mode {
            FilePickerMode::OpenFile => primary.clone(),
            FilePickerMode::SaveFile => {
                Self::valid_filename(&self.filename).then(|| FilePickerEvent::SaveFile {
                    name: self.filename.clone(),
                })
            }
            FilePickerMode::SelectDirectory => Some(FilePickerEvent::ChooseDirectory {
                directory: self.directory.clone(),
            }),
        };
        if self.mode == FilePickerMode::SaveFile {
            let mut input = TextInputView::themed(
                &self.theme,
                self.filename.clone(),
                self.families.clone(),
                self.filename_action,
            );
            input.placeholder = "文件名".into();
            input.disabled = self.disabled;
            input.invalid = !self.filename.is_empty() && !Self::valid_filename(&self.filename);
            views.push(View::new(input).keyed("filename"));
        }
        let mut buttons = Vec::new();
        if self.mode != FilePickerMode::OpenFile
            && let Some(FilePickerEvent::EnterDirectory(id)) = primary
        {
            let mut button =
                ButtonView::themed(&self.theme, "进入目录", self.families.clone(), self.action);
            button.disabled = self.disabled;
            buttons.push(View::new(ActionButtonView {
                button,
                payload: FilePickerEvent::EnterDirectory(id),
            }));
        }
        let mut button = ButtonView::themed(
            &self.theme,
            match self.mode {
                FilePickerMode::OpenFile => "打开",
                FilePickerMode::SaveFile => "保存",
                FilePickerMode::SelectDirectory => "选择此目录",
            },
            self.families.clone(),
            self.action,
        );
        button.disabled = self.disabled || payload.is_none();
        buttons.push(View::new(ActionButtonView {
            button,
            payload: payload.unwrap_or(FilePickerEvent::Parent),
        }));
        views.push(row(buttons).style(|s| s.gap = Size::new(8.0, 0.0)).build());
        Ok(column(views).style(|s| s.gap = Size::new(0.0, 8.0)).build())
    }
}
#[derive(Clone)]
struct FileListView {
    list: ListBoxView,
    entries: Vec<FileEntry>,
    allow_files: bool,
}
struct FileList {
    properties: FileListView,
    list: ListBox,
}
impl ViewSpec for FileListView {
    type Widget = FileList;
    fn build(&self) -> FileList {
        FileList {
            properties: self.clone(),
            list: self.list.build(),
        }
    }
    fn update(&self, w: &mut FileList) -> Invalidation {
        let result = self.list.update(&mut w.list);
        w.properties = self.clone();
        result
    }
}
impl Widget for FileList {
    fn enabled(&self) -> bool {
        self.list.enabled()
    }
    fn focusable(&self) -> bool {
        self.list.focusable()
    }
    fn measure(&mut self, c: MeasureContext) -> Result<Size, LayoutError> {
        self.list.measure(c)
    }
    fn prepare(&mut self, c: PrepareContext) -> Result<(), LayoutError> {
        self.list.prepare(c)
    }
    fn paint(&self, c: PaintContext, p: &mut Painter<'_>) {
        self.list.paint(c, p)
    }
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        if self.enabled()
            && c.phase == EventPhase::Target
            && matches!(
                e,
                WidgetEvent::Input(InputEvent::Key(KeyInput {
                    key: Key::Enter,
                    pressed: true,
                    repeat: false,
                    ..
                }))
            )
            && let Some(entry) = self.list.active_item().and_then(|id| {
                self.properties
                    .entries
                    .iter()
                    .find(|e| e.id.0 == id.0 && !e.disabled)
            })
        {
            let payload = if entry.directory {
                Some(FilePickerEvent::EnterDirectory(entry.id))
            } else if self.properties.allow_files {
                Some(FilePickerEvent::OpenFile(entry.id))
            } else {
                None
            };
            if let Some(payload) = payload {
                return EventResponse {
                    handled: true,
                    actions: alloc::vec![WidgetAction::new(self.properties.list.action, payload)],
                    ..Default::default()
                };
            }
        }
        let mut r = self.list.event(c, e);
        for a in &mut r.actions {
            if let Some(event) = a.payload::<ListSelectionChanged>() {
                *a = WidgetAction::new(
                    a.id,
                    FilePickerEvent::Selected(FileEntryId(event.selected.0)),
                );
            }
        }
        r
    }
}
