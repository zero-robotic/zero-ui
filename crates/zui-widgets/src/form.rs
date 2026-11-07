//! Form presentation only: applications own validation and field values.
use crate::{TextView, column, grid};
use alloc::{string::String, vec::Vec};
use zui_api::{
    layout::{Dimension, Edges, Track},
    view::View,
};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::Size;
pub struct FormField {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub help: Option<String>,
    pub error: Option<String>,
    pub content: View,
}
impl FormField {
    pub fn new(key: impl Into<String>, label: impl Into<String>, content: View) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            required: false,
            help: None,
            error: None,
            content,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FormLayout {
    Inline { label_width: f32 },
    Stacked,
}
pub struct FormView {
    pub theme: Theme,
    pub families: Vec<String>,
    pub layout: FormLayout,
}
impl FormView {
    pub fn themed(theme: &Theme, families: Vec<String>) -> Self {
        Self {
            theme: theme.clone(),
            families,
            layout: FormLayout::Inline { label_width: 120.0 },
        }
    }
    pub fn build(self, fields: Vec<FormField>) -> View {
        let mut rows = Vec::new();
        for field in fields {
            let mut label = TextView::themed(
                &self.theme,
                if field.required {
                    alloc::format!("{} *", field.label)
                } else {
                    field.label
                },
                self.families.clone(),
                TypeRole::Body,
            );
            if field.error.is_some() {
                label.color = self.theme.color(ColorRole::Danger);
            }
            let mut content = alloc::vec![field.content.keyed("editor")];
            if let Some(help) = field.help {
                let mut text =
                    TextView::themed(&self.theme, help, self.families.clone(), TypeRole::Caption);
                text.color = self.theme.color(ColorRole::TextMuted);
                content.push(View::new(text).keyed("help"));
            }
            if let Some(error) = field.error {
                let mut text =
                    TextView::themed(&self.theme, error, self.families.clone(), TypeRole::Caption);
                text.color = self.theme.color(ColorRole::Danger);
                content.push(View::new(text).keyed("error"));
            }
            let body = column(content)
                .style(|s| {
                    s.min_width = Dimension::Length(0.0);
                    s.gap = Size::new(0.0, 4.0);
                })
                .build();
            let views = [View::new(label).keyed("label"), body.keyed("body")];
            let row = match self.layout {
                FormLayout::Inline { label_width } => {
                    grid([Track::Length(label_width), Track::Fraction(1.0)], views)
                        .style(|s| s.gap = Size::new(12.0, 0.0))
                        .build()
                }
                FormLayout::Stacked => column(views).style(|s| s.gap = Size::new(0.0, 6.0)).build(),
            };
            rows.push(row.keyed(field.key));
        }
        column(rows)
            .style(|s| {
                s.width = Dimension::Percent(1.0);
                s.gap = Size::new(0.0, 12.0);
                s.padding = Edges::all(4.0);
            })
            .build()
    }
}
/// Naming alias for APIs that use the View suffix for composition descriptions.
pub type FormFieldView = FormField;
