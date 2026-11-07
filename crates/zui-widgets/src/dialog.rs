//! Modal compositions reuse Popup's focus and dismissal contract.
use crate::{
    ButtonView, PopupPlacement, PopupView, ScrollView, TextView, action_button::ActionButtonView,
    column, row,
};
use alloc::{string::String, vec::Vec};
use zui_api::{
    action::ActionId,
    layout::{Dimension, Edges},
    view::View,
};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Color, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    Accept,
    Cancel,
    Yes,
    No,
}
pub struct DialogView {
    pub title: String,
    pub families: Vec<String>,
    pub theme: Theme,
    pub size: Size,
    pub dismiss: ActionId,
    pub close_on_outside: bool,
    /// Title and close affordance are independent. Escape dismissal stays available.
    pub show_title: bool,
    pub show_close_button: bool,
}
impl DialogView {
    pub fn themed(
        theme: &Theme,
        title: impl Into<String>,
        families: Vec<String>,
        dismiss: ActionId,
    ) -> Self {
        Self {
            title: title.into(),
            families,
            theme: theme.clone(),
            size: Size::new(440.0, 280.0),
            dismiss,
            close_on_outside: false,
            show_title: true,
            show_close_button: false,
        }
    }
    pub fn build(self, body: View, footer: View) -> View {
        let mut popup = PopupView::new(PopupPlacement::Center, self.size, self.dismiss);
        popup.backdrop = Color::rgba(0, 0, 0, 140);
        popup.close_on_outside = self.close_on_outside;
        let mut header = Vec::new();
        if self.show_title {
            header.push(
                View::new(TextView::themed(
                    &self.theme,
                    self.title,
                    self.families.clone(),
                    TypeRole::Heading,
                ))
                .keyed("dialog-title")
                .styled(zui_api::layout::LayoutStyle {
                    grow: 1.0,
                    min_width: Dimension::Length(0.0),
                    ..Default::default()
                }),
            );
        } else if self.show_close_button {
            header.push(crate::Container::empty().style(|s| s.grow = 1.0).build());
        }
        if self.show_close_button {
            let mut button =
                ButtonView::themed(&self.theme, "关闭", self.families.clone(), self.dismiss);
            button.style = crate::ButtonStyle::text(&self.theme);
            header.push(
                View::new(ActionButtonView {
                    button,
                    payload: crate::PopupDismissed {
                        reason: crate::DismissReason::CloseButton,
                    },
                })
                .keyed("dialog-close"),
            );
        }
        let mut children = Vec::new();
        if !header.is_empty() {
            children.push(
                row(header)
                    .style(|s| {
                        s.gap = Size::new(12.0, 0.0);
                        s.shrink = 0.0;
                    })
                    .build(),
            );
        }
        children.push(
            ScrollView::vertical(body)
                .focusable(false)
                .style(|s| {
                    s.grow = 1.0;
                    s.min_height = Dimension::Length(0.0);
                    s.basis = Dimension::Length(0.0);
                })
                .build()
                .keyed("dialog-body"),
        );
        let mut footer = footer;
        footer.style.shrink = 0.0;
        children.push(footer.keyed("dialog-footer"));
        popup.build(
            column(children)
                .background(self.theme.color(ColorRole::Surface))
                .border(self.theme.color(ColorRole::Border))
                .radius(self.theme.controls().panel_radius)
                .style(|s| {
                    s.padding = Edges::all(12.0);
                    s.gap = Size::new(0.0, 8.0);
                })
                .build(),
        )
    }
}
pub struct MessageBoxView {
    pub dialog: DialogView,
    pub text: String,
    pub buttons: Vec<(String, DialogResult)>,
    pub action: ActionId,
}
impl MessageBoxView {
    pub fn themed(
        theme: &Theme,
        title: impl Into<String>,
        text: impl Into<String>,
        families: Vec<String>,
        action: ActionId,
        dismiss: ActionId,
    ) -> Self {
        Self {
            dialog: DialogView::themed(theme, title, families, dismiss),
            text: text.into(),
            buttons: alloc::vec![
                ("确定".into(), DialogResult::Accept),
                ("取消".into(), DialogResult::Cancel)
            ],
            action,
        }
    }
    pub fn build(self) -> View {
        let body = View::new(TextView::themed(
            &self.dialog.theme,
            self.text,
            self.dialog.families.clone(),
            TypeRole::Body,
        ));
        let buttons = self.buttons.into_iter().map(|(label, payload)| {
            let mut button = ButtonView::themed(
                &self.dialog.theme,
                label,
                self.dialog.families.clone(),
                self.action,
            );
            if matches!(payload, DialogResult::Accept | DialogResult::Yes) {
                button.style = crate::ButtonStyle::primary(&self.dialog.theme);
            }
            View::new(ActionButtonView { button, payload }).styled(zui_api::layout::LayoutStyle {
                min_width: Dimension::Length(80.0),
                padding: Edges::all(if self.dialog.theme.controls().min_height > 0 {
                    0.0
                } else {
                    10.0
                }),
                ..Default::default()
            })
        });
        let footer = row(buttons)
            .style(|s| {
                s.wrap = true;
                s.justify = zui_api::layout::Justify::End;
                s.gap = Size::new(12.0, 8.0);
            })
            .build();
        self.dialog.build(body, footer)
    }
}
