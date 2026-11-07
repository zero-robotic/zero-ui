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
        }
    }
    pub fn build(self, body: View, footer: View) -> View {
        let mut popup = PopupView::new(PopupPlacement::Center, self.size, self.dismiss);
        popup.backdrop = Color::rgba(0, 0, 0, 140);
        popup.close_on_outside = self.close_on_outside;
        let title = View::new(TextView::themed(
            &self.theme,
            self.title,
            self.families,
            TypeRole::Heading,
        ));
        popup.build(
            ScrollView::vertical(
                column([title, body, footer])
                    .style(|s| {
                        s.padding = Edges::all(20.0);
                        s.gap = Size::new(0.0, 16.0);
                    })
                    .build(),
            )
            .background(self.theme.color(ColorRole::Surface))
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
            View::new(ActionButtonView {
                button: ButtonView::themed(
                    &self.dialog.theme,
                    label,
                    self.dialog.families.clone(),
                    self.action,
                ),
                payload,
            })
            .styled(zui_api::layout::LayoutStyle {
                min_width: Dimension::Length(80.0),
                padding: Edges::all(10.0),
                ..Default::default()
            })
        });
        let footer = row(buttons)
            .style(|s| {
                s.wrap = true;
                s.gap = Size::new(12.0, 8.0);
            })
            .build();
        self.dialog.build(body, footer)
    }
}
