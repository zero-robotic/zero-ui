use std::collections::BTreeSet;
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges},
    view::View,
};
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::*;
#[derive(Clone)]
pub struct State {
    expanded: BTreeSet<TreeItemId>,
    selected: u64,
    sort: Option<TableSort>,
    ratio: f64,
    note: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            expanded: BTreeSet::from([TreeItemId(0)]),
            selected: 1,
            sort: None,
            ratio: 0.3,
            note: "设备运行正常\n在这里记录巡检结果，可换行、撤销和粘贴。".into(),
        }
    }
}
pub enum Message {
    Tree(TreeEvent),
    Table(TableEvent),
    Split(f64),
    Note(String),
}
impl State {
    pub fn map(&self, e: &ActionEvent) -> Option<Message> {
        match e.action.id {
            ActionId(200) => e.action.payload::<TreeEvent>().copied().map(Message::Tree),
            ActionId(201) => e
                .action
                .payload::<TableEvent>()
                .copied()
                .map(Message::Table),
            ActionId(202) => e
                .action
                .payload::<SplitterChanged>()
                .map(|c| Message::Split(c.ratio)),
            ActionId(203) => match e.action.payload::<TextInputEvent>() {
                Some(TextInputEvent::Changed(s)) => Some(Message::Note(s.clone())),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn apply(&mut self, m: Message) {
        match m {
            Message::Tree(TreeEvent::Selected(id)) => {
                if id.0 > 0 {
                    self.selected = id.0;
                }
            }
            Message::Tree(TreeEvent::Expansion { id, expanded }) => {
                if expanded {
                    self.expanded.insert(id);
                } else {
                    self.expanded.remove(&id);
                }
            }
            Message::Table(TableEvent::Selected(id)) => {
                self.selected = id.0;
                self.expanded.insert(TreeItemId(0));
            }
            Message::Table(TableEvent::SortRequested(sort)) => self.sort = Some(sort),
            Message::Split(ratio) => self.ratio = ratio,
            Message::Note(note) => self.note = note,
        }
    }
    pub fn view(&self, theme: &Theme, f: &[String]) -> View {
        let mut group = TreeItem::new(TreeItemId(0), "设备分组");
        group.children = (1..=3)
            .map(|id| TreeItem::new(TreeItemId(id), format!("设备 {id}")))
            .collect();
        let mut tree = TreeView::themed(
            theme,
            vec![group],
            f.to_vec(),
            Some(TreeItemId(self.selected)),
            ActionId(200),
        );
        tree.expanded = self.expanded.clone();
        let mut rows: Vec<_> = (1..=3)
            .map(|id| {
                TableRow::new(
                    TableRowId(id),
                    vec![
                        format!("设备 {id}"),
                        format!("{}%", id * 25),
                        "运行中".into(),
                    ],
                )
            })
            .collect();
        if let Some(sort) = self.sort {
            rows.sort_by_key(|r| r.cells[sort.column.0 as usize].clone());
            if sort.direction == SortDirection::Descending {
                rows.reverse();
            }
        }
        let mut table = TableView::themed(
            theme,
            vec![
                TableColumn::new(TableColumnId(0), "名称", 140.0),
                TableColumn::new(TableColumnId(1), "负载", 100.0),
                TableColumn::new(TableColumnId(2), "状态", 120.0),
            ],
            rows,
            f.to_vec(),
            ActionId(201),
        );
        table.selected = Some(TableRowId(self.selected));
        table.sort = self.sort;
        let mut split =
            SplitterView::themed(theme, self.ratio, SplitAxis::Horizontal, ActionId(202))
                .build(tree.build().unwrap(), table.build().unwrap());
        split.style.width = Dimension::Percent(1.0);
        split.style.height = Dimension::Length(230.0);
        let mut note = TextAreaView::themed(theme, self.note.clone(), f.to_vec(), ActionId(203));
        note.editor.placeholder = "输入多行巡检记录".into();
        column([
            View::new(TextView::themed(
                theme,
                "数据视图：树与表格共享设备选择；表头排序；拖动分栏",
                f.to_vec(),
                TypeRole::Body,
            )),
            split,
            View::new(TextView::themed(
                theme,
                format!(
                    "当前设备 {} · 分栏 {:.0}% · 记录 {} 字",
                    self.selected,
                    self.ratio * 100.0,
                    self.note.chars().count()
                ),
                f.to_vec(),
                TypeRole::Caption,
            )),
            View::new(note).styled(zui_api::layout::LayoutStyle {
                width: Dimension::Percent(1.0),
                height: Dimension::Length(130.0),
                padding: Edges::all(8.0),
                ..Default::default()
            }),
        ])
        .style(|s| s.gap = Size::new(0.0, 10.0))
        .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_table_sort_split_and_note_share_application_state() {
        let mut state = State::default();
        state.apply(Message::Tree(TreeEvent::Selected(TreeItemId(3))));
        assert_eq!(state.selected, 3);
        state.apply(Message::Tree(TreeEvent::Expansion {
            id: TreeItemId(0),
            expanded: false,
        }));
        assert!(state.expanded.is_empty());
        state.apply(Message::Table(TableEvent::Selected(TableRowId(2))));
        assert_eq!(state.selected, 2);
        assert!(state.expanded.contains(&TreeItemId(0)));
        state.apply(Message::Table(TableEvent::SortRequested(TableSort {
            column: TableColumnId(1),
            direction: SortDirection::Descending,
        })));
        assert_eq!(state.sort.unwrap().direction, SortDirection::Descending);
        state.apply(Message::Split(0.65));
        assert_eq!(state.ratio, 0.65);
        state.apply(Message::Note("第一行\n第二行".into()));
        assert_eq!(state.note.lines().count(), 2);
    }
}
