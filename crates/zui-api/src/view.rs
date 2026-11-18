//! Declarative descriptions contain properties; persistent widgets own local state.
use crate::{
    layout::LayoutStyle,
    widget::{Invalidation, LayoutTag, Widget},
};
use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::any::{Any, TypeId};

/// Unique among siblings, not globally. Text and numeric keys are distinct.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Number(u64),
    Text(String),
}
impl From<u64> for Key {
    fn from(value: u64) -> Self {
        Self::Number(value)
    }
}
impl From<&str> for Key {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}
impl From<String> for Key {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

/// Opaque association for upper-layer view owners. Does not affect matching,
/// layout, or widget behavior. Clones identify the same association.
#[derive(Clone, Debug, Default)]
pub struct ViewMarker(Arc<()>);
impl PartialEq for ViewMarker {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ViewMarker {}

/// Implement alongside a widget to separate current properties from local state.
/// `build` is called only for new instances. `update` must preserve local state
/// unless a changed property makes that state invalid, and report its invalidation.
/// Callbacks are infallible; validate fallible application data before creating views.
pub trait ViewSpec: 'static {
    type Widget: Widget;
    fn build(&self) -> Self::Widget;
    fn update(&self, widget: &mut Self::Widget) -> Invalidation;
}

trait ErasedSpec {
    fn widget_type(&self) -> TypeId;
    fn build(&self) -> Box<dyn Widget>;
    fn update(&self, widget: &mut dyn Widget) -> Option<Invalidation>;
}
impl<V: ViewSpec> ErasedSpec for V {
    fn widget_type(&self) -> TypeId {
        TypeId::of::<V::Widget>()
    }
    fn build(&self) -> Box<dyn Widget> {
        Box::new(ViewSpec::build(self))
    }
    fn update(&self, widget: &mut dyn Widget) -> Option<Invalidation> {
        (widget as &mut dyn Any)
            .downcast_mut::<V::Widget>()
            .map(|widget| ViewSpec::update(self, widget))
    }
}

/// Source of the child descriptions for a container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChildOwnership {
    #[default]
    Declarative,
    Widget,
}

/// A fresh description can be produced from application state on every update.
/// Unkeyed children match by absolute sibling position. Keyed children match by
/// key within the same reused parent. A changed widget type replaces its subtree.
pub struct View {
    pub marker: Option<ViewMarker>,
    pub key: Option<Key>,
    pub style: LayoutStyle,
    pub clip: bool,
    pub children: Vec<View>,
    pub child_ownership: ChildOwnership,
    pub layout_tag: Option<LayoutTag>,
    spec: Box<dyn ErasedSpec>,
}
impl View {
    pub fn new(spec: impl ViewSpec) -> Self {
        Self {
            marker: None,
            key: None,
            style: LayoutStyle::default(),
            clip: false,
            children: Vec::new(),
            child_ownership: ChildOwnership::Declarative,
            layout_tag: None,
            spec: Box::new(spec),
        }
    }
    pub fn keyed(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
    /// Parent-owned association for layout feedback; does not alter node identity.
    pub fn layout_tagged(mut self, tag: LayoutTag) -> Self {
        self.layout_tag = Some(tag);
        self
    }
    pub fn styled(mut self, style: LayoutStyle) -> Self {
        self.style = style;
        self
    }
    pub fn clipped(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }
    pub fn with_children(mut self, children: impl IntoIterator<Item = View>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
    /// Let the persistent widget describe children after layout. Explicit children
    /// must be empty. Matching owners retain their realized children on rebuild.
    pub fn managed_children(mut self) -> Self {
        self.child_ownership = ChildOwnership::Widget;
        self
    }
    /// Runtime bridge; contains no concrete widget recognition logic.
    pub fn widget_type(&self) -> TypeId {
        self.spec.widget_type()
    }
    pub fn build_widget(&self) -> Box<dyn Widget> {
        self.spec.build()
    }
    /// Returns None on a type mismatch, without touching the widget.
    pub fn update_widget(&self, widget: &mut dyn Widget) -> Option<Invalidation> {
        self.spec.update(widget)
    }
}
