use crate::{ButtonView, Icon, IconKind, IconView, action_button::ActionButtonView, row};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use zui_api::{
    action::ActionId,
    layout::{Edges, LayoutStyle},
    view::View,
};
use zui_theme::Theme;
use zui_types::Size;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationError {
    DuplicateId,
    InvalidPage,
}
impl core::fmt::Display for NavigationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "navigation error: {self:?}")
    }
}
impl core::error::Error for NavigationError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BreadcrumbSelected {
    pub id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbItem {
    pub id: u64,
    pub label: String,
    pub disabled: bool,
}
impl BreadcrumbItem {
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
        }
    }
}
pub struct BreadcrumbView {
    pub items: Vec<BreadcrumbItem>,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
}
impl BreadcrumbView {
    pub fn themed(
        theme: &Theme,
        items: Vec<BreadcrumbItem>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            items,
            theme: theme.clone(),
            families,
            action,
        }
    }
    pub fn build(self) -> Result<View, NavigationError> {
        let mut ids = BTreeSet::new();
        if self.items.iter().any(|i| !ids.insert(i.id)) {
            return Err(NavigationError::DuplicateId);
        }
        let count = self.items.len();
        let mut views = Vec::new();
        for (index, item) in self.items.into_iter().enumerate() {
            if index > 0 {
                views.push(
                    View::new(IconView::themed(
                        &self.theme,
                        Icon::builtin(IconKind::ChevronRight),
                    ))
                    .keyed(alloc::format!("sep-{}", item.id)),
                );
            }
            let mut button =
                ButtonView::themed(&self.theme, item.label, self.families.clone(), self.action);
            button.disabled = item.disabled || index + 1 == count;
            views.push(
                View::new(ActionButtonView {
                    button,
                    payload: BreadcrumbSelected { id: item.id },
                })
                .keyed(item.id)
                .styled(LayoutStyle {
                    padding: Edges::all(6.0),
                    ..Default::default()
                }),
            );
        }
        Ok(row(views)
            .style(|s| {
                s.wrap = true;
                s.gap = Size::new(4.0, 4.0);
            })
            .build())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageChanged {
    pub page: u64,
}
pub struct PaginationView {
    pub page: u64,
    pub page_count: u64,
    pub disabled: bool,
    pub theme: Theme,
    pub families: Vec<String>,
    pub action: ActionId,
}
impl PaginationView {
    pub fn themed(
        theme: &Theme,
        page: u64,
        page_count: u64,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        Self {
            page,
            page_count,
            disabled: false,
            theme: theme.clone(),
            families,
            action,
        }
    }
    pub fn build(self) -> Result<View, NavigationError> {
        if self.page_count == 0 || self.page == 0 || self.page > self.page_count {
            return Err(NavigationError::InvalidPage);
        }
        let make = |page: u64, label: String, disabled: bool, selected: bool| {
            let mut button =
                ButtonView::themed(&self.theme, label, self.families.clone(), self.action);
            button.style = if selected {
                let mut style = crate::ButtonStyle::primary(&self.theme);
                if !self.disabled {
                    style.disabled = style.normal;
                }
                style
            } else {
                crate::ButtonStyle::secondary(&self.theme)
            };
            button.disabled = self.disabled || disabled;
            View::new(ActionButtonView {
                button,
                payload: PageChanged { page },
            })
            .styled(LayoutStyle {
                padding: Edges::all(if self.theme.controls().min_height > 0 {
                    0.0
                } else {
                    8.0
                }),
                ..Default::default()
            })
        };
        let start = self
            .page
            .saturating_sub(2)
            .max(1)
            .min(self.page_count.saturating_sub(4).max(1));
        let end = start.saturating_add(4).min(self.page_count);
        let mut views = alloc::vec![
            make(1, "首页".into(), self.page == 1, false).keyed("first"),
            make(
                self.page.saturating_sub(1).max(1),
                "上一页".into(),
                self.page == 1,
                false
            )
            .keyed("previous")
        ];
        for page in start..=end {
            views.push(
                make(
                    page,
                    alloc::format!("{page}"),
                    page == self.page,
                    page == self.page,
                )
                .keyed(page),
            );
        }
        views.push(
            make(
                self.page.saturating_add(1).min(self.page_count),
                "下一页".into(),
                self.page == self.page_count,
                false,
            )
            .keyed("next"),
        );
        views.push(
            make(
                self.page_count,
                "末页".into(),
                self.page == self.page_count,
                false,
            )
            .keyed("last"),
        );
        Ok(row(views)
            .style(|s| {
                s.wrap = true;
                s.gap = Size::new(4.0, 4.0);
            })
            .build())
    }
}
