//! Cross-layer tests use only public APIs, keeping core independent of widgets.
use super::*;
use std::{cell::Cell, rc::Rc, time::Duration};
use zui_app::{ViewApplication, ViewUpdate, reactive::Region};
use zui_platform::{WindowEvent, WindowMetrics};

struct Page {
    values: [Property<u64>; 2],
    moved: Property<bool>,
    hidden: Property<bool>,
    regions: [Region; 2],
    builds: [Rc<Cell<usize>>; 3],
}
impl Component for Page {
    fn view(&self) -> View {
        self.builds[0].set(self.builds[0].get() + 1);
        let moved = self.moved.get();
        let hidden = self.hidden.get();
        column([
            column(if !moved && !hidden {
                vec![self.regions[0].view()]
            } else {
                vec![]
            })
            .build(),
            column(if moved && !hidden {
                vec![self.regions[0].view()]
            } else {
                vec![]
            })
            .build(),
            self.regions[1].view(),
        ])
        .build()
    }
    fn update(&mut self, _: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        self.values[0].set(99).unwrap();
        ViewAction::rebuild()
    }
}
fn app() -> ReactiveApplication<Page> {
    ReactiveApplication::new(|scope, _| {
        let values = [scope.property(1), scope.property(2)];
        let builds = std::array::from_fn(|_| Rc::new(Cell::new(0)));
        let regions = std::array::from_fn(|index| {
            let value = values[index].clone();
            let builds = builds[index + 1].clone();
            Region::new(move || {
                builds.set(builds.get() + 1);
                View::new(TextView::themed(
                    &Theme::desktop_dark(),
                    value.get().to_string(),
                    vec![],
                    TypeRole::Body,
                ))
                .keyed(index as u64)
            })
        });
        Page {
            values,
            moved: scope.property(false),
            hidden: scope.property(false),
            regions,
            builds,
        }
    })
}
fn flush(app: &ReactiveApplication<Page>, tree: &mut zui_core::Tree) -> bool {
    match app.view_update(tree) {
        ViewUpdate::Root(view) => {
            tree.reconcile(view).unwrap();
            false
        }
        ViewUpdate::Subtree { root, view } => {
            tree.reconcile_subtree(root, view).unwrap();
            true
        }
    }
}
#[test]
fn one_region_updates_without_building_parent_or_sibling() {
    let app = app();
    let mut tree = zui_core::Tree::from_view(app.view(), Default::default()).unwrap();
    app.component().values[0].set(10).unwrap();
    assert!(app.view_is_dirty());
    assert!(flush(&app, &mut tree));
    assert_eq!(
        app.component().builds.each_ref().map(|n| n.get()),
        [1, 2, 1]
    );
    assert!(!app.view_is_dirty());
    app.component().values[0].set(11).unwrap();
    app.component().values[1].set(12).unwrap();
    assert!(!flush(&app, &mut tree));
    assert_eq!(
        app.component().builds.each_ref().map(|n| n.get()),
        [2, 3, 2]
    );
}
#[test]
fn moving_hiding_and_remounting_regions_preserves_page_state() {
    let app = app();
    let mut tree = zui_core::Tree::from_view(app.view(), Default::default()).unwrap();
    app.component().values[0].set(42).unwrap();
    app.component().moved.set(true).unwrap();
    assert!(!flush(&app, &mut tree));
    app.component().values[0].set(43).unwrap();
    assert!(flush(&app, &mut tree));
    app.component().hidden.set(true).unwrap();
    flush(&app, &mut tree);
    app.component().values[0].set(44).unwrap();
    assert!(!app.view_is_dirty());
    app.component().hidden.set(false).unwrap();
    flush(&app, &mut tree);
    assert_eq!(app.component().values[0].get(), 44);
    assert!(!app.view_is_dirty());
    app.component().values[0].set(45).unwrap();
    assert!(flush(&app, &mut tree));
}
#[test]
fn explicit_rebuild_is_not_lost_when_a_region_is_also_dirty() {
    let mut app = app();
    let mut tree = zui_core::Tree::from_view(app.view(), Default::default()).unwrap();
    app.update(
        WindowEvent::Wake,
        WindowMetrics {
            extent: zui_types::Extent::new(100, 100),
            scale: 1.0,
            occluded: false,
        },
        Duration::ZERO,
    );
    assert!(!flush(&app, &mut tree));
    assert_eq!(app.component().builds[0].get(), 2);
}

#[test]
fn nested_region_reads_do_not_subscribe_parent_and_parent_updates_cover_children() {
    struct Nested {
        region: Region,
        value: Property<u64>,
        visible: Property<bool>,
        counts: [Rc<Cell<u32>>; 2],
    }
    impl Component for Nested {
        fn view(&self) -> View {
            column([self.region.view()]).build()
        }
    }
    let app = ReactiveApplication::new(|scope, _| {
        let value = scope.property(1);
        let visible = scope.property(true);
        let counts: [Rc<Cell<u32>>; 2] = std::array::from_fn(|_| Rc::new(Cell::new(0)));
        let child = {
            let (value, count) = (value.clone(), counts[1].clone());
            Region::new(move || {
                count.set(count.get() + 1);
                View::new(TextView::themed(
                    &Theme::desktop_dark(),
                    value.get().to_string(),
                    vec![],
                    TypeRole::Body,
                ))
            })
        };
        let region = {
            let (visible, count) = (visible.clone(), counts[0].clone());
            Region::new(move || {
                count.set(count.get() + 1);
                column(if visible.get() {
                    vec![child.view()]
                } else {
                    vec![]
                })
                .build()
            })
        };
        Nested {
            region,
            value,
            visible,
            counts,
        }
    });
    let mut tree = zui_core::Tree::from_view(app.view(), Default::default()).unwrap();
    app.component().value.set(2).unwrap();
    let ViewUpdate::Subtree { root, view } = app.view_update(&tree) else {
        panic!("child should update independently")
    };
    tree.reconcile_subtree(root, view).unwrap();
    assert_eq!(app.component().counts.each_ref().map(|c| c.get()), [1, 2]);
    app.component().value.set(3).unwrap();
    app.component().visible.set(false).unwrap();
    let ViewUpdate::Subtree { root, view } = app.view_update(&tree) else {
        panic!("parent should cover dirty child")
    };
    tree.reconcile_subtree(root, view).unwrap();
    assert_eq!(app.component().counts.each_ref().map(|c| c.get()), [2, 2]);
    assert!(!app.view_is_dirty());
    app.component().value.set(4).unwrap();
    assert!(!app.view_is_dirty());
}
