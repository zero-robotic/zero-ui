use crate::WidgetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticRole {
    Generic,
    Text,
    Button,
    TextInput,
    Group,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticsNode {
    pub id: WidgetId,
    pub role: SemanticRole,
    pub label: String,
    pub enabled: bool,
    pub children: Vec<SemanticsNode>,
}

impl SemanticsNode {
    pub fn new(id: WidgetId, role: SemanticRole, label: impl Into<String>) -> Self {
        Self {
            id,
            role,
            label: label.into(),
            enabled: true,
            children: Vec::new(),
        }
    }
}
