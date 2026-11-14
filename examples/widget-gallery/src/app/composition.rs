//! Application-owned state coordinates independent controls through public actions.
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges, LayoutStyle, Track},
    view::View,
};
use zui_paint::{ImageLimits, ImageResource, Path};
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Extent, Rect, Size};
use zui_widgets::*;

#[derive(Clone)]
pub(super) struct Extras {
    expanded: bool,
    selection: u64,
    accordion: AccordionExpansion,
    image: ImageResource,
    revision: u64,
    multi: std::collections::BTreeSet<ListItemId>,
}
pub(super) enum Message {
    Expand(bool),
    Select(u64),
    Accordion(AccordionExpansion),
    Multi(std::collections::BTreeSet<ListItemId>),
}
impl Extras {
    pub fn new() -> Self {
        let mut rgba = Vec::new();
        for y in 0..32 {
            for x in 0..64 {
                rgba.extend_from_slice(if (x / 8 + y / 8) % 2 == 0 {
                    &[30, 210, 240, 255]
                } else {
                    &[230, 130, 40, 255]
                });
            }
        }
        Self {
            expanded: true,
            selection: 1,
            accordion: AccordionExpansion::Single(Some(AccordionId(1))),
            image: ImageResource::from_rgba8(Extent::new(64, 32), rgba, ImageLimits::default())
                .unwrap(),
            revision: 0,
            multi: std::collections::BTreeSet::from([ListItemId(1)]),
        }
    }
    pub fn map_action(&self, e: &ActionEvent) -> Option<Message> {
        match e.action.id {
            ActionId(17) => e
                .action
                .payload::<ListMultiSelectionChanged>()
                .map(|e| Message::Multi(e.selected.clone())),
            ActionId(10) => e
                .action
                .payload::<ToggleChanged>()
                .map(|e| Message::Expand(e.selected)),
            ActionId(11) => e
                .action
                .payload::<SwitchChanged>()
                .map(|e| Message::Expand(e.checked)),
            ActionId(12) => e
                .action
                .payload::<DisclosureChanged>()
                .map(|e| Message::Expand(e.expanded)),
            ActionId(13) => e
                .action
                .payload::<SegmentChanged>()
                .map(|e| Message::Select(e.selected.0)),
            ActionId(14) => e
                .action
                .payload::<ListSelectionChanged>()
                .map(|e| Message::Select(e.selected.0)),
            ActionId(15) => e
                .action
                .payload::<TabChanged>()
                .map(|e| Message::Select(e.selected.0)),
            ActionId(16) => e
                .action
                .payload::<AccordionChanged>()
                .map(|e| Message::Accordion(e.expansion.clone())),
            _ => None,
        }
    }
    pub fn message(&mut self, m: Message) {
        match m {
            Message::Multi(set) => self.multi = set,
            Message::Expand(value) => self.expanded = value,
            Message::Select(value) => {
                self.selection = value;
                self.revision += 1;
            }
            Message::Accordion(value) => self.accordion = value,
        }
    }
    pub fn view(&self, theme: &Theme, families: &[String], note: &str, level: f64) -> View {
        let text = |value: String| {
            View::new(TextView::themed(
                theme,
                value,
                families.to_vec(),
                TypeRole::Body,
            ))
        };
        let selection = self.selection;
        let families_owned = families.to_vec();
        let list_theme = theme.clone();
        let virtual_list = VirtualListView::new(1000, 36.0, move |index| {
            View::new(TextView::themed(
                &list_theme,
                format!("虚拟行 {:04} · 当前方案 {selection}", index + 1),
                families_owned.clone(),
                TypeRole::Body,
            ))
            .keyed(format!("record-{index}"))
        })
        .scroll_to_index(
            (self.selection as usize - 1) * 400,
            ScrollAlignment::Start,
            self.revision,
        )
        .style(|s| {
            s.height = Dimension::Length(180.0);
            s.width = Dimension::Percent(1.0);
        })
        .build();
        let tabs = TabsView::themed(
            theme,
            (1..=3)
                .map(|n| TabOption::new(TabId(n), format!("方案 {n}")))
                .collect(),
            families.to_vec(),
            Some(TabId(self.selection)),
            ActionId(15),
        )
        .build(|id| {
            text(format!(
                "页签 {} · 输入框回显：{} · 当前设定 {:.0}%",
                id.0, note, level
            ))
        })
        .unwrap();
        let mut image = ImageView::new(self.image.clone());
        image.fit = match self.selection {
            1 => ImageFit::Contain,
            2 => ImageFit::Cover,
            _ => ImageFit::Fill,
        };
        let canvas = CanvasView::new(Size::new(240.0, 64.0), move |ctx, p| {
            let r = ctx.content_bounds;
            p.fill(
                Path::rounded_rectangle(r, 12.0).unwrap(),
                Color::rgb(15, 40, 60),
            );
            p.fill_rect(
                Rect::new(
                    r.origin.x + 8.0,
                    r.origin.y + 24.0,
                    (r.size.width - 16.0).max(0.0) * level / 100.0,
                    16.0,
                ),
                Color::rgb(30, 210, 240),
            );
        });
        let mut multi = ListBoxView::themed(
            theme,
            (1..=8)
                .map(|i| ListItem::new(ListItemId(i), format!("多选任务 {i}")))
                .collect(),
            families.to_vec(),
            None,
            ActionId(17),
        );
        multi.multi_selection = Some(self.multi.clone());
        column([
            text("ToggleButton / Switch / Disclosure：共享展开状态".into()),
            row([
                View::new(ToggleButtonView::themed(
                    theme,
                    "显示详情",
                    families.to_vec(),
                    self.expanded,
                    ActionId(10),
                ))
                .styled(LayoutStyle {
                    padding: Edges::all(10.0),
                    ..Default::default()
                }),
                View::new(SwitchView::themed(theme, self.expanded, ActionId(11))),
            ])
            .style(|s| s.gap = Size::new(16.0, 8.0))
            .build(),
            DisclosureView::themed(
                theme,
                "Disclosure · 可从上方开关控制",
                families.to_vec(),
                self.expanded,
                ActionId(12),
            )
            .build(|| text(format!("详情内容 · 文本输入同步：{note}"))),
            text("SegmentedControl / ListBox / Tabs：互相同步，驱动图片模式和虚拟列表定位".into()),
            View::new(SegmentedControlView::themed(
                theme,
                (1..=3)
                    .map(|n| SegmentOption::new(SegmentId(n), format!("方案 {n}")))
                    .collect(),
                families.to_vec(),
                Some(SegmentId(self.selection)),
                ActionId(13),
            )),
            View::new(ListBoxView::themed(
                theme,
                (1..=3)
                    .map(|n| ListItem::new(ListItemId(n), format!("方案 {n}")))
                    .collect(),
                families.to_vec(),
                Some(ListItemId(self.selection)),
                ActionId(14),
            ))
            .styled(LayoutStyle {
                height: Dimension::Length(120.0),
                ..Default::default()
            }),
            tabs,
            text("Image：方案 1 包含 / 2 覆盖 / 3 拉伸；Canvas 跟随 Slider / Stepper".into()),
            grid(
                [Track::Fraction(1.0), Track::Fraction(1.0)],
                [
                    View::new(image).styled(LayoutStyle {
                        width: Dimension::Percent(1.0),
                        height: Dimension::Length(100.0),
                        ..Default::default()
                    }),
                    stack([View::new(canvas), text(format!("Canvas · {:.0}%", level))]).build(),
                ],
            )
            .style(|s| s.gap = Size::new(16.0, 8.0))
            .build(),
            text("VirtualList：1000 行，选择方案定位到第 1 / 401 / 801 行".into()),
            virtual_list,
            text(format!("ListBox 多选：已选 {} 项；Ctrl/Command 点击切换，Shift 范围选择，Ctrl/Command+A 全选",self.multi.len())),
            View::new(multi).styled(LayoutStyle {height:Dimension::Length(160.0),..Default::default()}),
            AccordionView::themed(
                theme,
                vec![
                    AccordionItem::new(AccordionId(1), "Accordion · 状态摘要"),
                    AccordionItem::new(AccordionId(2), "Accordion · 操作提示"),
                ],
                families.to_vec(),
                self.accordion.clone(),
                ActionId(16),
            )
            .build(|id| {
                text(if id.0 == 1 {
                    format!("方案 {} / 设定 {:.0}% / {}", self.selection, level, note)
                } else {
                    "鼠标滚轮滚动；Tab 切换焦点；方向键操作选择控件。所有联动由应用状态统一驱动。"
                        .into()
                })
            })
            .unwrap(),
        ])
        .style(|s| {
            s.padding = Edges::all(24.0);
            s.gap = Size::new(0.0, 16.0);
            s.width = Dimension::Percent(1.0);
        })
        .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_share_selection_and_expansion_state() {
        let mut state = Extras::new();
        for (id, action, expected) in [
            (
                10,
                zui_api::action::WidgetAction::new(ActionId(10), ToggleChanged { selected: false }),
                false,
            ),
            (
                11,
                zui_api::action::WidgetAction::new(ActionId(11), SwitchChanged { checked: true }),
                true,
            ),
            (
                12,
                zui_api::action::WidgetAction::new(
                    ActionId(12),
                    DisclosureChanged { expanded: false },
                ),
                false,
            ),
        ] {
            let message = state
                .map_action(&ActionEvent {
                    source: zui_types::NodeId(1),
                    action,
                })
                .unwrap();
            state.message(message);
            assert_eq!(state.expanded, expected, "action {id}");
        }
        for action in [
            zui_api::action::WidgetAction::new(
                ActionId(13),
                SegmentChanged {
                    selected: SegmentId(2),
                },
            ),
            zui_api::action::WidgetAction::new(
                ActionId(14),
                ListSelectionChanged {
                    selected: ListItemId(2),
                },
            ),
            zui_api::action::WidgetAction::new(ActionId(15), TabChanged { selected: TabId(2) }),
        ] {
            state.selection = 1;
            let message = state
                .map_action(&ActionEvent {
                    source: zui_types::NodeId(1),
                    action,
                })
                .unwrap();
            state.message(message);
            assert_eq!(state.selection, 2);
        }
    }
}
