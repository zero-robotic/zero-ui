use crate::charts::SparklineView;
use alloc::{string::String, vec::Vec};
use zui_api::{
    layout::{Align, Dimension, Edges, LayoutStyle},
    text::TextStyle,
    view::{Key, View},
};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Color, Size};
use zui_widgets::{Container, TextView, column, row};
/// Semantic assessment supplied by the application, independent of numeric sign.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetricTone {
    #[default]
    Neutral,
    Positive,
    Warning,
    Negative,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricChange {
    pub text: String,
    pub tone: MetricTone,
}
impl MetricChange {
    pub fn new(text: impl Into<String>, tone: MetricTone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct MetricCardStyle {
    pub background: Color,
    pub accent: Color,
    pub title: Color,
    pub value: Color,
    pub unit: Color,
    pub neutral: Color,
    pub positive: Color,
    pub warning: Color,
    pub negative: Color,
    pub title_text: TextStyle,
    pub value_text: TextStyle,
    pub detail_text: TextStyle,
}
impl MetricCardStyle {
    pub fn from_theme(theme: &Theme, families: Vec<String>) -> Self {
        let text = |role| TextView::themed(theme, "", families.clone(), role).style;
        Self {
            background: theme.color(ColorRole::Surface),
            accent: theme.color(ColorRole::Accent),
            title: theme.color(ColorRole::TextMuted),
            value: theme.color(ColorRole::Text),
            unit: theme.color(ColorRole::TextMuted),
            neutral: theme.color(ColorRole::TextMuted),
            positive: theme.color(ColorRole::Success),
            warning: theme.color(ColorRole::Warning),
            negative: theme.color(ColorRole::Danger),
            title_text: text(TypeRole::Body),
            value_text: text(TypeRole::Display),
            detail_text: text(TypeRole::Caption),
        }
    }
    fn tone(&self, tone: MetricTone) -> Color {
        match tone {
            MetricTone::Neutral => self.neutral,
            MetricTone::Positive => self.positive,
            MetricTone::Warning => self.warning,
            MetricTone::Negative => self.negative,
        }
    }
}
/// Passive metric composition. Formatting, units and business meaning stay in the model.
pub struct MetricCardView {
    pub title: String,
    pub value: String,
    pub unit: Option<String>,
    pub change: Option<MetricChange>,
    pub trend: Option<SparklineView>,
    pub style: MetricCardStyle,
    pub layout: LayoutStyle,
    pub key: Option<Key>,
}
impl MetricCardView {
    pub fn themed(
        theme: &Theme,
        title: impl Into<String>,
        value: impl Into<String>,
        families: Vec<String>,
    ) -> Self {
        Self {
            title: title.into(),
            value: value.into(),
            unit: None,
            change: None,
            trend: None,
            style: MetricCardStyle::from_theme(theme, families),
            layout: LayoutStyle {
                padding: Edges::all(16.0),
                gap: Size::new(0.0, 12.0),
                ..Default::default()
            },
            key: None,
        }
    }
    pub fn build(self) -> View {
        let s = &self.style;
        let mut value = Vec::with_capacity(2);
        value.push(
            View::new(TextView::new(self.value, s.value_text.clone(), s.value)).keyed("value"),
        );
        if let Some(unit) = self.unit {
            value.push(View::new(TextView::new(unit, s.detail_text.clone(), s.unit)).keyed("unit"));
        }
        let mut children = Vec::with_capacity(5);
        children.push(
            Container::empty()
                .background(s.accent)
                .keyed("accent")
                .style(|s| {
                    s.height = Dimension::Length(3.0);
                    s.shrink = 0.0;
                })
                .build(),
        );
        children.push(
            View::new(TextView::new(self.title, s.title_text.clone(), s.title)).keyed("title"),
        );
        children.push(
            row(value)
                .keyed("reading")
                .style(|s| {
                    s.wrap = true;
                    s.align = Align::End;
                    s.gap = Size::new(6.0, 4.0);
                })
                .build(),
        );
        if let Some(change) = self.change {
            children.push(
                View::new(TextView::new(
                    change.text,
                    s.detail_text.clone(),
                    s.tone(change.tone),
                ))
                .keyed("change"),
            );
        }
        if let Some(trend) = self.trend {
            children.push(View::new(trend).keyed("trend"));
        }
        let mut root = column(children)
            .background(s.background)
            .style(|s| *s = self.layout)
            .build();
        root.key = self.key;
        root
    }
}
