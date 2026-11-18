//! Single-threaded UI state. Independent of widgets, trees and platform services.
//!
//! Create properties once in a component's scope; observe reads while building
//! its view. Writes only change versions: consumers decide when to refresh, so
//! several writes naturally coalesce without dropping operation events.
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

thread_local! {
    static READS: RefCell<Vec<Vec<Dependency>>> = const { RefCell::new(Vec::new()) };
    static COMPUTING: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct Dependency {
    name: Option<Rc<str>>,
    revision: Weak<Cell<u64>>,
    seen: u64,
}

/// Owns property writes and cleanup registrations, independently of layout.
pub struct Scope {
    lifetime: Rc<Lifetime>,
}
struct Lifetime {
    active: Cell<bool>,
    next: Cell<u64>,
    cleanups: RefCell<BTreeMap<u64, Box<dyn FnOnce()>>>,
}
impl Default for Scope {
    fn default() -> Self {
        Self {
            lifetime: Rc::new(Lifetime {
                active: Cell::new(true),
                next: Cell::new(0),
                cleanups: RefCell::new(BTreeMap::new()),
            }),
        }
    }
}
impl Scope {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn token(&self) -> ScopeToken {
        ScopeToken(Rc::downgrade(&self.lifetime))
    }
    pub fn named_property<T>(&self, name: impl Into<String>, value: T) -> Property<T> {
        let mut property = self.property(value);
        property.name = Some(Rc::from(name.into()));
        property
    }
    pub fn property<T>(&self, value: T) -> Property<T> {
        Property {
            name: None,
            value: Rc::new(RefCell::new(value)),
            revision: Rc::new(Cell::new(0)),
            owner: self.token(),
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        self.lifetime.active.set(false);
        let cleanups = std::mem::take(&mut *self.lifetime.cleanups.borrow_mut());
        for cleanup in cleanups.into_values() {
            cleanup();
        }
    }
}
/// Weak lifetime capability; retaining it does not keep a component alive.
#[derive(Clone)]
pub struct ScopeToken(Weak<Lifetime>);
impl ScopeToken {
    pub fn is_alive(&self) -> bool {
        self.0.upgrade().is_some_and(|life| life.active.get())
    }
    /// Register cleanup and keep the returned registration as long as needed.
    /// Dropping the registration unregisters it without executing the cleanup.
    /// If already disposed, cleanup executes immediately.
    pub fn on_dispose(&self, cleanup: impl FnOnce() + 'static) -> Disposal {
        if let Some(life) = self.0.upgrade()
            && life.active.get()
        {
            let id = life.next.get();
            life.next
                .set(id.checked_add(1).expect("scope registration IDs exhausted"));
            life.cleanups.borrow_mut().insert(id, Box::new(cleanup));
            Disposal {
                scope: self.clone(),
                id,
            }
        } else {
            cleanup();
            Disposal {
                scope: self.clone(),
                id: 0,
            }
        }
    }
}
pub struct Disposal {
    scope: ScopeToken,
    id: u64,
}
impl Drop for Disposal {
    fn drop(&mut self) {
        if let Some(life) = self.scope.0.upgrade() {
            let removed = life.cleanups.borrow_mut().remove(&self.id);
            drop(removed);
        }
    }
}

/// A stale handle remains readable, but cannot write after its owner is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
    Disposed,
    DuringObservation,
}
impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WriteError {}

pub struct Property<T> {
    name: Option<Rc<str>>,
    value: Rc<RefCell<T>>,
    revision: Rc<Cell<u64>>,
    owner: ScopeToken,
}
impl<T> Clone for Property<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            value: self.value.clone(),
            revision: self.revision.clone(),
            owner: self.owner.clone(),
        }
    }
}
impl<T: Clone + 'static> Property<T> {
    pub fn get(&self) -> T {
        READS.with_borrow_mut(|frames| {
            if let Some(reads) = frames.last_mut() {
                reads.push(Dependency {
                    name: self.name.clone(),
                    revision: Rc::downgrade(&self.revision),
                    seen: self.revision.get(),
                });
            }
        });
        self.value.borrow().clone()
    }
    /// Expose observation without granting write authority.
    pub fn read_only(&self) -> Read<T> {
        let property = self.clone();
        Read(Rc::new(move || property.get()))
    }
}
impl<T: PartialEq> Property<T> {
    pub fn set(&self, value: T) -> Result<bool, WriteError> {
        if !self.owner.is_alive() {
            return Err(WriteError::Disposed);
        }
        if READS.with_borrow(|frames| !frames.is_empty())
            || COMPUTING.with_borrow(|stack| !stack.is_empty())
        {
            return Err(WriteError::DuringObservation);
        }
        let mut current = self.value.borrow_mut();
        if *current == value {
            return Ok(false);
        }
        *current = value;
        self.revision.set(self.revision.get().wrapping_add(1));
        Ok(true)
    }
}

/// Read-only source, including an adapter to an external observable model.
/// Adapters must read tracked properties to participate in invalidation.
pub struct Read<T>(Rc<dyn Fn() -> T>);
impl<T> Clone for Read<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: 'static> Read<T> {
    pub fn get(&self) -> T {
        (self.0)()
    }
    pub fn map<U: 'static>(&self, map: impl Fn(T) -> U + 'static) -> Computed<U> {
        let source = self.clone();
        Computed::new(move || map(source.get()))
    }
}

/// Pure, read-only derivation. Dependencies are discovered on each read,
/// including conditional dependencies. No side effects or writes are allowed.
/// Recursive computations panic with a cycle diagnostic instead of overflowing.
pub struct Computed<T>(Read<T>);
impl<T> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: 'static> Computed<T> {
    pub fn new(compute: impl Fn() -> T + 'static) -> Self {
        let identity = Rc::new(());
        Self(Read(Rc::new(move || {
            let id = Rc::as_ptr(&identity) as usize;
            COMPUTING.with_borrow_mut(|stack| {
                assert!(!stack.contains(&id), "cycle in computed UI state");
                stack.push(id);
            });
            struct Pop;
            impl Drop for Pop {
                fn drop(&mut self) {
                    COMPUTING.with_borrow_mut(|stack| {
                        stack.pop();
                    });
                }
            }
            let _guard = Pop;
            compute()
        })))
    }
    pub fn get(&self) -> T {
        self.0.get()
    }
    pub fn read_only(&self) -> Read<T> {
        self.0.clone()
    }
}

/// Diagnostic explanation of a changed read dependency. Names are optional and
/// never participate in identity, lookup, or binding behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateChange {
    pub name: Option<String>,
    pub previous_version: u64,
    pub current_version: Option<u64>,
}

/// Read dependencies of one view/component. Holds weak references only.
#[derive(Default)]
pub struct Observer {
    dependencies: Vec<Dependency>,
}
impl Observer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn changes(&self) -> Vec<StateChange> {
        self.dependencies
            .iter()
            .filter_map(|dependency| {
                let current = dependency.revision.upgrade().map(|value| value.get());
                (current != Some(dependency.seen)).then(|| StateChange {
                    name: dependency.name.as_ref().map(ToString::to_string),
                    previous_version: dependency.seen,
                    current_version: current,
                })
            })
            .collect()
    }
    pub fn is_dirty(&self) -> bool {
        self.dependencies.iter().any(|dependency| {
            dependency
                .revision
                .upgrade()
                .is_none_or(|revision| revision.get() != dependency.seen)
        })
    }
    /// Replace dependencies only after a successful build. Panic unwinding
    /// restores tracking and keeps the previous successful dependencies.
    pub fn observe<T>(&mut self, build: impl FnOnce() -> T) -> T {
        self.track(build, true)
    }
    /// Observe an independently refreshed component without subscribing its
    /// parent to the component's reads. The caller must refresh both observers.
    pub fn observe_isolated<T>(&mut self, build: impl FnOnce() -> T) -> T {
        self.track(build, false)
    }
    fn track<T>(&mut self, build: impl FnOnce() -> T, propagate: bool) -> T {
        READS.with_borrow_mut(|frames| frames.push(Vec::new()));
        struct Frame(bool);
        impl Drop for Frame {
            fn drop(&mut self) {
                if self.0 {
                    READS.with_borrow_mut(|frames| {
                        frames.pop();
                    });
                }
            }
        }
        let mut frame = Frame(true);
        let value = build();
        let mut dependencies = READS.with_borrow_mut(|frames| frames.pop().unwrap());
        frame.0 = false;
        dependencies.sort_by_key(|dependency| dependency.revision.as_ptr());
        dependencies.dedup_by_key(|dependency| dependency.revision.as_ptr());
        // Nested observation still contributes reads to its enclosing view.
        READS.with_borrow_mut(|frames| {
            if propagate && let Some(parent) = frames.last_mut() {
                parent.extend(dependencies.iter().cloned());
            }
        });
        self.dependencies = dependencies;
        value
    }
}

#[cfg(test)]
mod tests;
