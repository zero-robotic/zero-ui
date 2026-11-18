use super::*;

#[test]
fn shared_value_batches_writes_and_ignores_equal_writes() {
    let scope = Scope::new();
    let value = scope.property(10);
    let editor = value.clone();
    let mut observer = Observer::new();
    assert_eq!(observer.observe(|| value.get()), 10);
    assert!(!editor.set(10).unwrap());
    assert!(!observer.is_dirty());
    editor.set(20).unwrap();
    editor.set(30).unwrap();
    assert!(observer.is_dirty());
    assert_eq!(observer.observe(|| value.get()), 30);
    assert!(!observer.is_dirty());
}

#[test]
fn conditional_derived_dependencies_are_replaced() {
    let scope = Scope::new();
    let selected = scope.property(true);
    let first = scope.property(1);
    let second = scope.property(2);
    let computed = {
        let (selected, first, second) = (selected.clone(), first.clone(), second.clone());
        Computed::new(move || {
            if selected.get() {
                first.get()
            } else {
                second.get()
            }
        })
    };
    let mut observer = Observer::new();
    assert_eq!(observer.observe(|| computed.get()), 1);
    second.set(3).unwrap();
    assert!(!observer.is_dirty());
    selected.set(false).unwrap();
    assert!(observer.is_dirty());
    assert_eq!(observer.observe(|| computed.get()), 3);
    first.set(4).unwrap();
    assert!(!observer.is_dirty());
}

#[test]
fn scope_disposes_write_access_without_layout_ownership() {
    let scope = Scope::new();
    let property = scope.property(1);
    let read = property.read_only();
    drop(scope);
    assert_eq!(property.set(2), Err(WriteError::Disposed));
    assert_eq!(read.get(), 1);
}

#[test]
fn observation_is_pure_and_recovers_from_unwind() {
    let scope = Scope::new();
    let value = scope.property(1);
    let mut observer = Observer::new();
    observer.observe(|| {
        assert_eq!(value.set(2), Err(WriteError::DuringObservation));
        value.get()
    });
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observer.observe(|| panic!("failed view"));
    }));
    assert!(failed.is_err());
    value.set(3).unwrap();
    assert!(observer.is_dirty());
}

#[test]
fn nested_observers_track_dependencies_without_retaining_values() {
    let scope = Scope::new();
    let value = scope.property(1);
    let weak = Rc::downgrade(&value.value);
    let mut parent = Observer::new();
    let mut child = Observer::new();
    parent.observe(|| child.observe(|| value.get()));
    value.set(2).unwrap();
    assert!(parent.is_dirty());
    assert!(child.is_dirty());
    drop(value);
    assert!(weak.upgrade().is_none());
}

#[test]
fn computed_cycle_is_reported_and_tracking_recovers() {
    let slot: Rc<RefCell<Option<Computed<i32>>>> = Rc::default();
    let weak = Rc::downgrade(&slot);
    let computed = Computed::new(move || weak.upgrade().unwrap().borrow().as_ref().unwrap().get());
    *slot.borrow_mut() = Some(computed.clone());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| computed.get())).is_err());
    let scope = Scope::new();
    assert!(scope.property(0).set(1).unwrap());
}

#[test]
fn isolated_observation_does_not_subscribe_parent() {
    let scope = Scope::new();
    let value = scope.property(1);
    let mut parent = Observer::new();
    let mut child = Observer::new();
    parent.observe(|| child.observe_isolated(|| value.get()));
    value.set(2).unwrap();
    assert!(!parent.is_dirty());
    assert!(child.is_dirty());
}

#[test]
fn disposing_scope_cleans_up_and_registrations_do_not_accumulate() {
    let scope = Scope::new();
    let token = scope.token();
    for _ in 0..1000 {
        let registration = token.on_dispose(|| panic!("unregistered cleanup"));
        drop(registration);
    }
    assert!(scope.lifetime.cleanups.borrow().is_empty());
    let called = Rc::new(Cell::new(false));
    let mark = called.clone();
    let _registration = token.on_dispose(move || mark.set(true));
    drop(scope);
    assert!(called.get());
    assert!(!token.is_alive());
}

#[test]
fn diagnostics_identify_changed_named_dependencies() {
    let scope = Scope::new();
    let value = scope.named_property("font size", 18);
    let other = scope.named_property("unchanged", 1);
    let mut observer = Observer::new();
    observer.observe(|| (value.get(), other.get()));
    value.set(24).unwrap();
    assert_eq!(
        observer.changes(),
        vec![StateChange {
            name: Some("font size".into()),
            previous_version: 0,
            current_version: Some(1),
        }]
    );
}
