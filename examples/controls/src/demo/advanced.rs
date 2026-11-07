use std::collections::BTreeSet;
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::Dimension,
    view::View,
};
use zui_theme::{Theme, TypeRole};
use zui_types::{Rect, Size};
use zui_widgets::*;
#[derive(Clone)]
pub struct State {
    query: String,
    chosen: BTreeSet<ListItemId>,
    open: bool,
    pub anchor: Option<Rect>,
    tabs: Vec<TabOption>,
    tab: Option<TabId>,
    nodes: Vec<TreeItem>,
    expanded: BTreeSet<TreeItemId>,
    selected: Option<TreeItemId>,
    selection: BTreeSet<TreeItemId>,
    status: String,
    grid_selected: Option<TableRowId>,
}
impl Default for State {
    fn default() -> Self {
        let mut node = TreeItem::new(TreeItemId(1), "展开加载设备");
        node.children_state = TreeChildrenState::Unloaded;
        Self {
            grid_selected: None,
            query: String::new(),
            chosen: BTreeSet::new(),
            open: false,
            anchor: None,
            tabs: (1..=3)
                .map(|id| TabOption::new(TabId(id), format!("页面 {id}")))
                .collect(),
            tab: Some(TabId(1)),
            nodes: vec![node, TreeItem::new(TreeItemId(2), "另一分组")],
            expanded: BTreeSet::new(),
            selected: Some(TreeItemId(1)),
            selection: BTreeSet::new(),
            status: "搜索并多选设备；拖动页签调整顺序；展开树节点加载数据".into(),
        }
    }
}
pub enum Message {
    GridSelected(TableRowId),
    Combo(ComboBoxEvent),
    Dismiss,
    Tab(TabId),
    Close(TabId),
    Reorder(TabReorderRequested),
    Tree(TreeEvent),
    Load(TreeItemId),
    Multi(TreeSelectionChanged),
    Drop(TreeDropRequested),
}
impl State {
    pub fn map(&self, e: &ActionEvent) -> Option<Message> {
        let a = &e.action;
        match a.id {
            ActionId(404) => a.payload::<DataGridEvent>().and_then(|e| {
                if let DataGridEvent::Selected(id) = e {
                    Some(Message::GridSelected(*id))
                } else {
                    None
                }
            }),
            ActionId(400) => a.payload::<ComboBoxEvent>().cloned().map(Message::Combo),
            ActionId(401) => a.payload::<PopupDismissed>().map(|_| Message::Dismiss),
            ActionId(402) => a
                .payload::<TabChanged>()
                .map(|e| Message::Tab(e.selected))
                .or_else(|| {
                    a.payload::<TabCloseRequested>()
                        .map(|e| Message::Close(e.id))
                })
                .or_else(|| {
                    a.payload::<TabReorderRequested>()
                        .copied()
                        .map(Message::Reorder)
                }),
            ActionId(403) => a
                .payload::<TreeEvent>()
                .copied()
                .map(Message::Tree)
                .or_else(|| {
                    a.payload::<TreeLoadRequested>()
                        .map(|e| Message::Load(e.id))
                })
                .or_else(|| {
                    a.payload::<TreeSelectionChanged>()
                        .cloned()
                        .map(Message::Multi)
                })
                .or_else(|| a.payload::<TreeDropRequested>().cloned().map(Message::Drop)),
            _ => None,
        }
    }
    pub fn apply(&mut self, m: Message) {
        match m {
            Message::GridSelected(id) => {
                self.grid_selected = Some(id);
                self.status = format!("选中虚拟表格记录 {}", id.0);
            }
            Message::Combo(ComboBoxEvent::Open) => self.open = true,
            Message::Combo(ComboBoxEvent::QueryChanged(s)) => self.query = s,
            Message::Combo(ComboBoxEvent::Submitted(s)) => {
                self.status = format!("自定义设备：{s}");
                self.open = false;
            }
            Message::Combo(ComboBoxEvent::SelectionChanged(ids)) => {
                self.status = format!("已选择 {} 台设备", ids.len());
                self.chosen = ids;
                self.open = false;
            }
            Message::Combo(ComboBoxEvent::Selected(id)) => {
                self.chosen = BTreeSet::from([id]);
                self.open = false;
            }
            Message::Dismiss => self.open = false,
            Message::Tab(id) => self.tab = Some(id),
            Message::Close(id) => {
                self.tabs.retain(|t| t.id != id);
                if self.tab == Some(id) {
                    self.tab = self.tabs.first().map(|t| t.id);
                }
            }
            Message::Reorder(e) => {
                if let Some(index) = self.tabs.iter().position(|t| t.id == e.id) {
                    let item = self.tabs.remove(index);
                    self.tabs.insert(e.to_index.min(self.tabs.len()), item);
                }
            }
            Message::Tree(TreeEvent::Selected(id)) => self.selected = Some(id),
            Message::Tree(TreeEvent::Expansion { id, expanded }) => {
                if expanded {
                    self.expanded.insert(id);
                } else {
                    self.expanded.remove(&id);
                }
            }
            Message::Load(id) => {
                if let Some(node) = find_mut(&mut self.nodes, id) {
                    node.children = (10..15)
                        .map(|i| TreeItem::new(TreeItemId(i), format!("设备 {i}")))
                        .collect();
                    node.children_state = TreeChildrenState::Loaded;
                    self.status = "已按展开请求加载演示设备".into();
                }
            }
            Message::Multi(e) => {
                self.status = format!("树已选择 {} 项", e.selected.len());
                self.selection = e.selected;
            }
            Message::Drop(e) => {
                // Only leaf moves are accepted by this demo's business policy.
                if !e.sources.contains(&e.target)
                    && e.sources.iter().all(|id| {
                        find_mut(&mut self.nodes, *id).is_some_and(|n| {
                            n.children.is_empty() && n.children_state == TreeChildrenState::Loaded
                        })
                    })
                    && find_mut(&mut self.nodes, e.target).is_some()
                {
                    let moved: Vec<_> = e
                        .sources
                        .iter()
                        .filter_map(|id| take(&mut self.nodes, *id))
                        .collect();
                    if let Some(target) = find_mut(&mut self.nodes, e.target) {
                        target.children.extend(moved);
                        self.expanded.insert(e.target);
                    }
                    self.status = "已移动所选叶节点".into();
                } else {
                    self.status = "此演示仅允许移动叶节点".into();
                }
            }
        }
    }
    fn combo(&self, t: &Theme, f: &[String]) -> ComboBoxView {
        let mut v = ComboBoxView::themed(
            t,
            (1..=20)
                .map(|i| ListItem::new(ListItemId(i), format!("设备 {i}")))
                .collect(),
            None,
            f.to_vec(),
            ActionId(400),
        );
        v.query = self.query.clone();
        v.selection = Some(self.chosen.clone());
        v.allow_custom = true;
        v
    }
    pub fn close_anchored(&mut self) {
        self.open = false;
        self.anchor = None;
    }
    pub fn overlay(&self, t: &Theme, f: &[String]) -> Option<View> {
        let anchor = self.anchor?;
        self.open
            .then(|| self.combo(t, f).popup(anchor, 300.0, ActionId(401)))
    }
    pub fn view(&self, t: &Theme, f: &[String]) -> View {
        let mut tabs = TabsView::themed(t, self.tabs.clone(), f.to_vec(), self.tab, ActionId(402));
        tabs.closable = self.tabs.iter().map(|t| t.id).collect();
        tabs.reorderable = true;
        let tabs = tabs
            .build(|id| {
                View::new(TextView::themed(
                    t,
                    format!("页面 {}：可关闭、拖动排序", id.0),
                    f.to_vec(),
                    TypeRole::Body,
                ))
            })
            .unwrap();
        let mut tree = TreeView::themed(
            t,
            self.nodes.clone(),
            f.to_vec(),
            self.selected,
            ActionId(403),
        );
        tree.expanded = self.expanded.clone();
        tree.selection = Some(self.selection.clone());
        tree.draggable = true;
        let mut table = TableView::themed(
            t,
            vec![TableColumn::new(
                TableColumnId(1),
                "虚拟表格：10000 行",
                300.0,
            )],
            (0..10_000)
                .map(|id| TableRow::new(TableRowId(id), vec![format!("记录 {id}")]))
                .collect(),
            f.to_vec(),
            ActionId(404),
        );
        table.columns[0].sortable = false;
        table.selected = self.grid_selected;
        let mut grid = DataGridView::new(table);
        grid.resizable = false;
        grid.virtual_rows = Some(DataGridVirtualRows {
            estimated_height: 40.0,
            viewport_height: 160.0,
            revision: 0,
        });
        GroupBoxView::themed(t, "增强交互", f.to_vec()).build(
            column([
                self.combo(t, f).build().keyed("device-combo"),
                tabs,
                tree.build().unwrap().styled(zui_api::layout::LayoutStyle {
                    height: Dimension::Length(200.0),
                    ..Default::default()
                }),
                grid.build().unwrap(),
                View::new(TextView::themed(
                    t,
                    &self.status,
                    f.to_vec(),
                    TypeRole::Body,
                )),
            ])
            .style(|s| s.gap = Size::new(0.0, 12.0))
            .build(),
        )
    }
}
fn find_mut(nodes: &mut [TreeItem], id: TreeItemId) -> Option<&mut TreeItem> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Some(n) = find_mut(&mut node.children, id) {
            return Some(n);
        }
    }
    None
}
fn take(nodes: &mut Vec<TreeItem>, id: TreeItemId) -> Option<TreeItem> {
    if let Some(i) = nodes.iter().position(|n| n.id == id) {
        return Some(nodes.remove(i));
    }
    for node in nodes {
        if let Some(n) = take(&mut node.children, id) {
            return Some(n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enhanced_gallery_applies_control_requests_and_layouts_popup() {
        let mut state = State::default();
        state.anchor = Some(Rect::new(24.0, 80.0, 300.0, 40.0));
        state.apply(Message::Combo(ComboBoxEvent::Open));
        assert!(state.open);
        state.apply(Message::Combo(ComboBoxEvent::SelectionChanged(
            BTreeSet::from([ListItemId(1), ListItemId(3)]),
        )));
        assert_eq!(state.chosen.len(), 2);
        assert!(!state.open);
        assert!(state.overlay(&Theme::dark(), &[]).is_none());
        state.apply(Message::Reorder(TabReorderRequested {
            id: TabId(1),
            to_index: 2,
        }));
        assert_eq!(state.tabs[2].id, TabId(1));
        state.apply(Message::Close(TabId(1)));
        assert_eq!(state.tab, Some(TabId(2)));
        state.apply(Message::Load(TreeItemId(1)));
        state.apply(Message::Drop(TreeDropRequested {
            sources: vec![TreeItemId(2)],
            target: TreeItemId(1),
        }));
        assert_eq!(state.nodes.len(), 1);
        assert!(
            state.nodes[0]
                .children
                .iter()
                .any(|n| n.id == TreeItemId(2))
        );
        state.apply(Message::GridSelected(TableRowId(9999)));
        assert_eq!(state.grid_selected, Some(TableRowId(9999)));
        state.anchor = Some(Rect::new(24.0, 80.0, 300.0, 40.0));
        state.apply(Message::Combo(ComboBoxEvent::Open));
        state.apply(Message::Combo(ComboBoxEvent::QueryChanged("1".into())));
        let mut text = zui_text::ParleyText::default();
        let f = text
            .register_font(std::sync::Arc::from(
                include_bytes!("../../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        for width in [480.0, 1100.0] {
            let t = Theme::dark();
            let mut root = stack([state.view(&t, &f), state.overlay(&t, &f).unwrap()]).build();
            root.style.width = Dimension::Length(width as f32);
            root.style.height = Dimension::Length(800.0);
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
