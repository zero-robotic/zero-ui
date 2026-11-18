//! Independent view observation scopes. Only this upper layer resolves markers
//! to retained nodes; core transports markers as uninterpreted associations.
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use zui_api::view::{View, ViewMarker};
use zui_core::Tree;
use zui_state::Observer;
use zui_types::NodeId;

thread_local! {
    static MOUNTS: RefCell<Vec<Vec<Weak<RegionInner>>>> = const { RefCell::new(Vec::new()) };
}

/// Optional independently refreshed UI region. Create it once with captured
/// property handles, then place `region.view()` in a component's description.
/// Put root styles/keys inside the builder so partial rebuilds preserve them.
/// One region may be mounted only once in a description. Moving it preserves
/// its external state, but follows ordinary retained Key rules for widget state.
#[derive(Clone)]
pub struct Region(Rc<RegionInner>);
pub(super) struct RegionInner {
    name: Option<String>,
    build: Box<dyn Fn() -> View>,
    observer: RefCell<Observer>,
    marker: ViewMarker,
    node: Cell<Option<NodeId>>,
    children: RefCell<Vec<Weak<RegionInner>>>,
}
impl Region {
    pub fn new(build: impl Fn() -> View + 'static) -> Self {
        Self(Rc::new(RegionInner {
            name: None,
            build: Box::new(build),
            observer: RefCell::new(Observer::new()),
            marker: ViewMarker::default(),
            node: Cell::new(None),
            children: RefCell::new(Vec::new()),
        }))
    }
    pub fn named(name: impl Into<String>, build: impl Fn() -> View + 'static) -> Self {
        let mut region = Self::new(build);
        Rc::get_mut(&mut region.0).unwrap().name = Some(name.into());
        region
    }
    pub fn view(&self) -> View {
        let view = self.0.render();
        MOUNTS.with_borrow_mut(|stack| {
            if let Some(mounts) = stack.last_mut() {
                mounts.push(Rc::downgrade(&self.0));
            }
        });
        view
    }
}
impl RegionInner {
    pub(super) fn render(&self) -> View {
        let (mut view, children) = collect(|| {
            self.observer
                .borrow_mut()
                .observe_isolated(|| (self.build)())
        });
        assert!(
            view.marker.is_none(),
            "nested regions need distinct root nodes"
        );
        view.marker = Some(self.marker.clone());
        *self.children.borrow_mut() = children;
        view
    }
}

pub(super) fn collect<T>(build: impl FnOnce() -> T) -> (T, Vec<Weak<RegionInner>>) {
    MOUNTS.with_borrow_mut(|stack| stack.push(Vec::new()));
    struct Pop(bool);
    impl Drop for Pop {
        fn drop(&mut self) {
            if self.0 {
                MOUNTS.with_borrow_mut(|stack| {
                    stack.pop();
                });
            }
        }
    }
    let mut guard = Pop(true);
    let value = build();
    let mounts = MOUNTS.with_borrow_mut(|stack| stack.pop().unwrap());
    guard.0 = false;
    let mut seen = Vec::new();
    validate(&mounts, &mut seen);
    (value, mounts)
}
fn validate(mounts: &[Weak<RegionInner>], seen: &mut Vec<*const RegionInner>) {
    for mount in mounts {
        assert!(
            !seen.contains(&mount.as_ptr()),
            "a region cannot be mounted more than once"
        );
        seen.push(mount.as_ptr());
        if let Some(region) = mount.upgrade() {
            validate(&region.children.borrow(), seen);
        }
    }
}
pub(super) fn is_dirty(mounts: &[Weak<RegionInner>]) -> bool {
    mounts.iter().any(|mount| {
        mount.upgrade().is_some_and(|region| {
            region.observer.borrow().is_dirty() || is_dirty(&region.children.borrow())
        })
    })
}
pub(super) fn dirty(mounts: &[Weak<RegionInner>], output: &mut Vec<Rc<RegionInner>>) {
    for mount in mounts {
        if let Some(region) = mount.upgrade() {
            if region.observer.borrow().is_dirty() {
                output.push(region);
            } else {
                dirty(&region.children.borrow(), output);
            }
        }
    }
}
pub(super) fn locate(tree: &Tree, region: &RegionInner) -> Option<NodeId> {
    if let Some(node) = region.node.get()
        && tree.view_marker(node).ok().flatten() == Some(&region.marker)
    {
        return Some(node);
    }
    let mut todo = vec![tree.root()];
    let mut found = None;
    while let Some(node) = todo.pop() {
        if tree.view_marker(node).ok().flatten() == Some(&region.marker) {
            if found.is_some() {
                return None;
            }
            found = Some(node);
        }
        todo.extend_from_slice(tree.children(node).ok()?);
    }
    region.node.set(found);
    found
}

/// Pending property changes for one independently observed region.
#[derive(Clone, Debug)]
pub struct RegionChanges {
    pub name: Option<String>,
    pub properties: Vec<zui_state::StateChange>,
}
pub(super) fn changes(mounts: &[Weak<RegionInner>], output: &mut Vec<RegionChanges>) {
    for mount in mounts {
        if let Some(region) = mount.upgrade() {
            let properties = region.observer.borrow().changes();
            if !properties.is_empty() {
                output.push(RegionChanges {
                    name: region.name.clone(),
                    properties,
                });
            }
            changes(&region.children.borrow(), output);
        }
    }
}
pub(super) fn validate_mounts(mounts: &[Weak<RegionInner>]) {
    validate(mounts, &mut Vec::new());
}
