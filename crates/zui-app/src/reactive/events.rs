use crate::ViewAction;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    marker::PhantomData,
    rc::Rc,
};
use zui_api::{
    action::{ActionEvent, ActionId, ActionView, ValueView},
    view::View,
};
use zui_state::{Disposal, Property, Scope, ScopeToken};

/// Stable typed route. The final clone dropping disconnects its handler;
/// disposing its scope disconnects it even if stale handles remain elsewhere.
pub struct Event<T> {
    id: ActionId,
    subscription: Rc<Subscription>,
    marker: PhantomData<fn(T)>,
}
struct Subscription {
    disconnect: Rc<dyn Fn()>,
    _disposal: Disposal,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        (self.disconnect)();
    }
}
impl<T> Clone for Event<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            subscription: self.subscription.clone(),
            marker: PhantomData,
        }
    }
}
impl<T> Event<T> {
    pub fn id(&self) -> ActionId {
        self.id
    }
    /// Build a spec whose primary action payload matches this route.
    pub fn view<V: ActionView<Action = T>>(&self, build: impl FnOnce(ActionId) -> V) -> View {
        let mut spec = build(self.id);
        spec.set_action(self.id);
        spec.into_view()
    }
}

/// One control kind connected to a shared property, which is authoritative both
/// initially and on subsequent refreshes. Create bindings outside view builds.
pub struct Binding<V: ValueView> {
    property: Property<V::Value>,
    event: Event<V::Action>,
}
impl<V: ValueView> Clone for Binding<V> {
    fn clone(&self) -> Self {
        Self {
            property: self.property.clone(),
            event: self.event.clone(),
        }
    }
}
impl<V: ValueView> Binding<V> {
    pub fn view(&self, build: impl FnOnce(V::Value, ActionId) -> V) -> View {
        let value = self.property.get();
        let mut spec = build(value.clone(), self.event.id());
        spec.set_value(value);
        spec.set_action(self.event.id());
        spec.into_view()
    }
}

type Handler<M> = dyn Fn(&mut M, &ActionEvent) -> Option<ViewAction>;
struct Registry<M> {
    next: Cell<u64>,
    handlers: RefCell<BTreeMap<ActionId, Rc<Handler<M>>>>,
}
/// Application-local event registration, reusable for dynamically created
/// components. Dispatch never holds a registry borrow while calling business code.
pub struct Events<M> {
    registry: Rc<Registry<M>>,
    scope: ScopeToken,
}
impl<M> Clone for Events<M> {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            scope: self.scope.clone(),
        }
    }
}
impl<M: 'static> Events<M> {
    pub(super) fn new(scope: &Scope) -> Self {
        Self {
            registry: Rc::new(Registry {
                next: Cell::new(0),
                handlers: RefCell::new(BTreeMap::new()),
            }),
            scope: scope.token(),
        }
    }
    /// Share routing while tying new registrations to a child component's scope.
    pub fn scoped(&self, scope: &Scope) -> Self {
        Self {
            registry: self.registry.clone(),
            scope: scope.token(),
        }
    }
    /// Diagnostic count, useful for detecting accidental per-build registration.
    pub fn subscription_count(&self) -> usize {
        self.registry.handlers.borrow().len()
    }
    pub fn binding<V: ValueView>(&self, property: &Property<V::Value>) -> Binding<V> {
        let target = property.clone();
        let event = self.on(move |_, action: &V::Action| {
            if let Some(value) = V::requested_value(action) {
                let _ = target.set(value);
            }
            ViewAction::default()
        });
        Binding {
            property: property.clone(),
            event,
        }
    }
    pub fn on<T: 'static>(&self, handler: impl Fn(&mut M, &T) -> ViewAction + 'static) -> Event<T> {
        let number = self.registry.next.get();
        self.registry
            .next
            .set(number.checked_add(1).expect("event IDs exhausted"));
        let id = ActionId(number);
        if self.scope.is_alive() {
            let owner = self.scope.clone();
            self.registry.handlers.borrow_mut().insert(
                id,
                Rc::new(move |model, event| {
                    if !owner.is_alive() {
                        return None;
                    }
                    event
                        .action
                        .payload::<T>()
                        .map(|payload| handler(model, payload))
                }),
            );
        }
        let weak = Rc::downgrade(&self.registry);
        let disconnect: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(registry) = weak.upgrade() {
                let removed = registry.handlers.borrow_mut().remove(&id);
                drop(removed);
            }
        });
        let cleanup = disconnect.clone();
        let disposal = self.scope.on_dispose(move || cleanup());
        Event {
            id,
            marker: PhantomData,
            subscription: Rc::new(Subscription {
                disconnect,
                _disposal: disposal,
            }),
        }
    }
    pub fn bind<T: 'static, V: Clone + PartialEq + 'static>(
        &self,
        property: &Property<V>,
        value: impl Fn(&T) -> V + 'static,
    ) -> Event<T> {
        let property = property.clone();
        self.on(move |_, payload| {
            // A property may belong to a shorter-lived scope than its route.
            // A late request to that property is intentionally ignored.
            let _ = property.set(value(payload));
            ViewAction::default()
        })
    }
    pub(super) fn dispatch(&self, model: &mut M, event: &ActionEvent) -> ViewAction {
        let handler = self
            .registry
            .handlers
            .borrow()
            .get(&event.action.id)
            .cloned();
        handler
            .and_then(|handler| handler(model, event))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_api::action::WidgetAction;
    use zui_types::NodeId;
    fn action(id: ActionId, value: impl std::any::Any) -> ActionEvent {
        ActionEvent {
            source: NodeId(1),
            action: WidgetAction::new(id, value),
        }
    }
    #[test]
    fn cloned_handlers_disconnect_on_last_drop_or_scope_disposal() {
        let root = Scope::new();
        let events = Events::<usize>::new(&root);
        let child = Scope::new();
        let scoped = events.scoped(&child);
        let captured = Rc::new(());
        let weak = Rc::downgrade(&captured);
        let route = scoped.on(move |model, value: &usize| {
            let _ = &captured;
            *model += value;
            ViewAction::default()
        });
        let clone = route.clone();
        let id = route.id();
        drop(route);
        let mut model = 0;
        events.dispatch(&mut model, &action(id, 2_usize));
        assert_eq!(model, 2);
        events.dispatch(&mut model, &action(id, "wrong payload"));
        assert_eq!(model, 2);
        drop(child);
        assert_eq!(events.subscription_count(), 0);
        assert!(weak.upgrade().is_none());
        events.dispatch(&mut model, &action(id, 10_usize));
        assert_eq!(model, 2);
        drop(clone);
        let next = events.on(|_, _: &usize| ViewAction::default());
        assert_ne!(id, next.id());
        drop(next);
        assert_eq!(events.subscription_count(), 0);
    }
    #[test]
    fn removing_a_handler_can_drop_another_subscription_without_borrow_conflicts() {
        let scope = Scope::new();
        let events = Events::<()>::new(&scope);
        let nested = events.on(|_, _: &()| ViewAction::default());
        let outer = events.on(move |_, _: &()| {
            let _ = &nested;
            ViewAction::default()
        });
        assert_eq!(events.subscription_count(), 2);
        drop(outer);
        assert_eq!(events.subscription_count(), 0);
    }
    #[test]
    fn repeated_registration_and_disposal_do_not_accumulate_handlers() {
        let scope = Scope::new();
        let events = Events::<()>::new(&scope);
        for _ in 0..1000 {
            let route = events.on(|_, _: &()| ViewAction::default());
            assert_eq!(events.subscription_count(), 1);
            drop(route);
        }
        assert_eq!(events.subscription_count(), 0);
    }
}
