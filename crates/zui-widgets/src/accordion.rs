//! Group expansion policy composes disclosures without involving the runtime.
use crate::{ButtonStyle, DisclosureView, column, disclosure::HeaderRequest};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    string::String,
    vec::Vec,
};
use core::cell::RefCell;
use zui_api::{
    action::{ActionId, WidgetAction},
    event::Key as InputKey,
    layout::LayoutStyle,
    text::TextStyle,
    view::{Key, View},
    widget::Lifecycle,
};
use zui_theme::Theme;
use zui_types::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccordionId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccordionItem {
    pub id: AccordionId,
    pub label: String,
    pub disabled: bool,
}
impl AccordionItem {
    pub fn new(id: AccordionId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
        }
    }
}
/// The variant defines policy; both policies allow every item to be collapsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccordionExpansion {
    Single(Option<AccordionId>),
    Multiple(BTreeSet<AccordionId>),
}
impl AccordionExpansion {
    pub fn contains(&self, id: AccordionId) -> bool {
        match self {
            Self::Single(selected) => *selected == Some(id),
            Self::Multiple(ids) => ids.contains(&id),
        }
    }
    fn toggled(&self, id: AccordionId) -> Self {
        match self {
            Self::Single(selected) => Self::Single(if *selected == Some(id) {
                None
            } else {
                Some(id)
            }),
            Self::Multiple(ids) => {
                let mut ids = ids.clone();
                if !ids.remove(&id) {
                    ids.insert(id);
                }
                Self::Multiple(ids)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccordionChanged {
    pub item: AccordionId,
    pub expansion: AccordionExpansion,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccordionError {
    DuplicateId(AccordionId),
    UnknownExpandedId(AccordionId),
}
impl core::fmt::Display for AccordionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "duplicate accordion ID {}", id.0),
            Self::UnknownExpandedId(id) => write!(f, "unknown expanded accordion ID {}", id.0),
        }
    }
}
impl core::error::Error for AccordionError {}
pub struct AccordionView {
    pub items: Vec<AccordionItem>,
    pub expansion: AccordionExpansion,
    pub disabled: bool,
    pub action: ActionId,
    pub text_style: TextStyle,
    pub header_style: ButtonStyle,
    pub layout: LayoutStyle,
    pub item_layout: LayoutStyle,
    pub header_layout: LayoutStyle,
    pub panel_layout: LayoutStyle,
    pub key: Option<Key>,
}
impl AccordionView {
    pub fn themed(
        theme: &Theme,
        items: Vec<AccordionItem>,
        families: Vec<String>,
        expansion: AccordionExpansion,
        action: ActionId,
    ) -> Self {
        let disclosure = DisclosureView::themed(theme, "", families, false, action);
        Self {
            items,
            expansion,
            disabled: false,
            action,
            text_style: disclosure.text_style,
            header_style: disclosure.header_style,
            layout: LayoutStyle::default(),
            item_layout: disclosure.layout,
            header_layout: disclosure.header_layout,
            panel_layout: disclosure.panel_layout,
            key: None,
        }
    }
    /// Validates every ID before invoking the factory, once per expanded item in display order.
    pub fn build(
        self,
        mut content: impl FnMut(AccordionId) -> View,
    ) -> Result<View, AccordionError> {
        let mut ids = BTreeSet::new();
        for item in &self.items {
            if !ids.insert(item.id) {
                return Err(AccordionError::DuplicateId(item.id));
            }
        }
        let validate = |id: &AccordionId| {
            if ids.contains(id) {
                Ok(())
            } else {
                Err(AccordionError::UnknownExpandedId(*id))
            }
        };
        match &self.expansion {
            AccordionExpansion::Single(Some(id)) => validate(id)?,
            AccordionExpansion::Multiple(ids) => {
                for id in ids {
                    validate(id)?;
                }
            }
            _ => {}
        }
        let navigation = Rc::new(HeaderNavigation {
            order: self
                .items
                .iter()
                .filter(|item| !self.disabled && !item.disabled)
                .map(|item| item.id)
                .collect(),
            nodes: RefCell::new(BTreeMap::new()),
        });
        let expansion = Rc::new(self.expansion);
        let mut children = Vec::with_capacity(self.items.len());
        for item in self.items {
            let request = ExpansionRequest {
                item: item.id,
                expansion: expansion.clone(),
                navigation: navigation.clone(),
            };
            let disclosure = DisclosureView {
                label: item.label,
                expanded: expansion.contains(item.id),
                disabled: self.disabled || item.disabled,
                action: self.action,
                text_style: self.text_style.clone(),
                header_style: self.header_style,
                layout: self.item_layout.clone(),
                header_layout: self.header_layout.clone(),
                panel_layout: self.panel_layout.clone(),
                key: Some(Key::Number(item.id.0)),
            };
            children.push(disclosure.build_with_request(|| content(item.id), request));
        }
        let mut view = column(children).style(|s| *s = self.layout).build();
        view.key = self.key;
        Ok(view)
    }
}

// All titles from one description share a single immutable snapshot. The owned
// public payload is materialized only when a user actually activates a title.
#[derive(Clone)]
struct ExpansionRequest {
    item: AccordionId,
    expansion: Rc<AccordionExpansion>,
    navigation: Rc<HeaderNavigation>,
}
impl PartialEq for ExpansionRequest {
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item && self.expansion == other.expansion
    }
}
#[derive(Default)]
struct HeaderNavigation {
    order: Vec<AccordionId>,
    nodes: RefCell<BTreeMap<AccordionId, NodeId>>,
}
impl HeaderNavigation {
    fn target(&self, from: AccordionId, key: &InputKey) -> Option<NodeId> {
        let index = self.order.iter().position(|id| *id == from)?;
        let next = match key {
            InputKey::ArrowDown => (index + 1) % self.order.len(),
            InputKey::ArrowUp => (index + self.order.len() - 1) % self.order.len(),
            InputKey::Home => 0,
            InputKey::End => self.order.len() - 1,
            _ => return None,
        };
        self.nodes.borrow().get(&self.order[next]).copied()
    }
}
impl HeaderRequest for ExpansionRequest {
    fn lifecycle(&self, id: NodeId, event: Lifecycle) {
        let mut nodes = self.navigation.nodes.borrow_mut();
        match event {
            Lifecycle::Mounted => {
                nodes.insert(self.item, id);
            }
            Lifecycle::Unmounted if nodes.get(&self.item) == Some(&id) => {
                nodes.remove(&self.item);
            }
            Lifecycle::Unmounted => {}
        }
    }
    fn navigate(&self, key: &InputKey) -> Option<NodeId> {
        self.navigation.target(self.item, key)
    }
    fn activate(&self, action: ActionId) -> WidgetAction {
        WidgetAction::new(
            action,
            AccordionChanged {
                item: self.item,
                expansion: self.expansion.toggled(self.item),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_share_snapshot_and_activation_produces_independent_owned_state() {
        let snapshot = Rc::new(AccordionExpansion::Multiple(
            [AccordionId(1), AccordionId(2)].into(),
        ));
        let requests: Vec<_> = (0..128)
            .map(|id| ExpansionRequest {
                item: AccordionId(id),
                expansion: snapshot.clone(),
                navigation: Rc::default(),
            })
            .collect();
        assert_eq!(Rc::strong_count(&snapshot), 129);
        for request in &requests {
            assert!(Rc::ptr_eq(&snapshot, &request.expansion));
        }
        let cloned = requests[1].clone();
        assert!(Rc::ptr_eq(&snapshot, &cloned.expansion));
        let action = cloned.activate(ActionId(9));
        assert_eq!(action.id, ActionId(9));
        let changed = action.payload::<AccordionChanged>().unwrap();
        assert_eq!(changed.item, AccordionId(1));
        assert!(!changed.expansion.contains(AccordionId(1)));
        assert!(changed.expansion.contains(AccordionId(2)));
        assert!(snapshot.contains(AccordionId(1)));
        assert_eq!(Rc::strong_count(&snapshot), 130);
    }
    #[test]
    fn request_equality_tracks_values_instead_of_new_snapshot_allocation() {
        let request = |state| ExpansionRequest {
            item: AccordionId(1),
            expansion: Rc::new(state),
            navigation: Rc::default(),
        };
        let a = request(AccordionExpansion::Single(Some(AccordionId(2))));
        let b = request(AccordionExpansion::Single(Some(AccordionId(2))));
        assert!(!Rc::ptr_eq(&a.expansion, &b.expansion));
        assert!(a == b); // Ordinary rebuilds must not cancel a pending gesture.
        assert!(a != request(AccordionExpansion::Single(None)));
        assert!(a != request(AccordionExpansion::Multiple([AccordionId(2)].into())));
    }
}
