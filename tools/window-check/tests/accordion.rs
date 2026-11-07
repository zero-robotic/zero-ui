use std::{sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    event::{InputEvent, Key, KeyInput},
    view::View,
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::Scene;
use zui_platform::{WindowEvent, WindowMetrics};
use zui_theme::Theme;
use zui_types::Extent;
use zui_widgets::{
    AccordionChanged, AccordionError, AccordionExpansion as Expansion, AccordionId as Id,
    AccordionItem, AccordionView, Container,
};
fn items() -> Vec<AccordionItem> {
    vec![
        AccordionItem::new(Id(1), "第一项"),
        AccordionItem::new(Id(2), "第二项"),
    ]
}
struct Model {
    families: Vec<String>,
    expansion: Expansion,
    reversed: bool,
}
impl ViewApplication for Model {
    type Message = Expansion;
    fn view(&self) -> View {
        let mut items = items();
        if self.reversed {
            items.reverse();
        }
        AccordionView::themed(
            &Theme::dark(),
            items,
            self.families.clone(),
            self.expansion.clone(),
            ActionId(1),
        )
        .build(|_| Container::empty().build())
        .unwrap()
    }
    fn map_action(&self, event: &ActionEvent) -> Option<Expansion> {
        assert_eq!(event.action.id, ActionId(1));
        event
            .action
            .payload::<AccordionChanged>()
            .map(|change| change.expansion.clone())
    }
    fn message(&mut self, expansion: Expansion) -> ViewAction {
        self.expansion = expansion;
        ViewAction::rebuild()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.reversed = !self.reversed;
            ViewAction::rebuild()
        } else {
            ViewAction::default()
        }
    }
}
#[test]
fn single_and_multiple_policy_round_trip_and_reorder_preserves_identity() {
    for multiple in [false, true] {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut host = WidgetHost::new(
            Model {
                families,
                expansion: if multiple {
                    Expansion::Multiple(Default::default())
                } else {
                    Expansion::Single(None)
                },
                reversed: false,
            },
            zui_layout::TaffyLayout::default(),
            text,
            Default::default(),
        )
        .unwrap();
        let metrics = WindowMetrics {
            extent: Extent::new(320, 240),
            scale: 1.0,
            occluded: false,
        };
        let key = |key| {
            WindowEvent::Input(InputEvent::Key(KeyInput {
                key,
                pressed: true,
                repeat: false,
                text: None,
                modifiers: Default::default(),
            }))
        };
        host.update(key(Key::Tab), metrics, Duration::ZERO).unwrap();
        host.update(key(Key::Enter), metrics, Duration::ZERO)
            .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert!(host.model().expansion.contains(Id(1)));
        let root = host.tree().root();
        let first = host.tree().children(root).unwrap()[0];
        let first_panel = host.tree().children(first).unwrap()[1];
        host.update(key(Key::Tab), metrics, Duration::ZERO).unwrap();
        host.update(key(Key::Enter), metrics, Duration::ZERO)
            .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert!(host.model().expansion.contains(Id(2)));
        assert_eq!(host.model().expansion.contains(Id(1)), multiple);
        assert_eq!(host.tree().contains(first_panel), multiple);
        let second = host.tree().children(root).unwrap()[1];
        let second_panel = host.tree().children(second).unwrap()[1];
        let focus = host.tree().focused();
        host.update(key(Key::Space), metrics, Duration::ZERO)
            .unwrap();
        host.update(WindowEvent::Wake, metrics, Duration::ZERO)
            .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert_eq!(host.tree().children(root).unwrap()[0], second);
        assert_eq!(host.tree().children(second).unwrap()[1], second_panel);
        assert_eq!(host.tree().focused(), focus);
        host.update(
            WindowEvent::Input(InputEvent::Key(KeyInput {
                key: Key::Space,
                pressed: false,
                repeat: false,
                text: None,
                modifiers: Default::default(),
            })),
            metrics,
            Duration::ZERO,
        )
        .unwrap();
        host.draw(&mut Scene::new(), metrics, Duration::ZERO)
            .unwrap();
        assert!(!host.model().expansion.contains(Id(2)));
        assert_eq!(host.model().expansion.contains(Id(1)), multiple);
    }
}
#[test]
fn validation_precedes_all_factories_and_content_follows_display_order() {
    let make = |items, expansion| {
        AccordionView::themed(&Theme::dark(), items, Vec::new(), expansion, ActionId(1))
    };
    let never = |_| -> View { panic!("invalid descriptions must not build content") };
    assert!(matches!(
        make(
            vec![
                AccordionItem::new(Id(1), "a"),
                AccordionItem::new(Id(1), "b")
            ],
            Expansion::Single(Some(Id(1)))
        )
        .build(never),
        Err(AccordionError::DuplicateId(Id(1)))
    ));
    for expansion in [
        Expansion::Single(Some(Id(9))),
        Expansion::Multiple([Id(1), Id(9)].into()),
    ] {
        assert!(matches!(
            make(items(), expansion).build(never),
            Err(AccordionError::UnknownExpandedId(Id(9)))
        ));
    }
    assert!(
        make(Vec::new(), Expansion::Single(None))
            .build(never)
            .is_ok()
    );
    let mut ordered = items();
    ordered.reverse();
    ordered[0].disabled = true;
    let mut built = Vec::new();
    make(ordered, Expansion::Multiple([Id(1), Id(2)].into()))
        .build(|id| {
            built.push(id);
            Container::empty().build()
        })
        .unwrap();
    assert_eq!(built, [Id(2), Id(1)]);
}

struct NavigationModel {
    families: Vec<String>,
    revision: usize,
}
impl ViewApplication for NavigationModel {
    type Message = ();
    fn view(&self) -> View {
        let theme = Theme::dark();
        let mut options: Vec<_> = (1..=4)
            .map(|id| {
                let mut item = AccordionItem::new(Id(id), format!("标题 {id}"));
                item.disabled = id % 2 == 0;
                item
            })
            .collect();
        if self.revision > 0 {
            options.reverse();
        }
        if self.revision > 1 {
            options.retain(|item| item.id != Id(3));
        }
        AccordionView::themed(
            &theme,
            options,
            self.families.clone(),
            Expansion::Single(Some(Id(1))),
            ActionId(1),
        )
        .build(|_| {
            View::new(zui_widgets::ButtonView::themed(
                &theme,
                "内容按钮",
                self.families.clone(),
                ActionId(2),
            ))
        })
        .unwrap()
    }
    fn map_action(&self, _: &ActionEvent) -> Option<()> {
        panic!("navigation must not activate or change expansion")
    }
    fn message(&mut self, _: ()) -> ViewAction {
        ViewAction::default()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.revision += 1;
            ViewAction::rebuild()
        } else {
            ViewAction::default()
        }
    }
}
#[test]
fn header_navigation_skips_disabled_wraps_and_tracks_reorder_and_removal() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut host = WidgetHost::new(
        NavigationModel {
            families,
            revision: 0,
        },
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap();
    let metrics = WindowMetrics {
        extent: Extent::new(320, 360),
        scale: 1.0,
        occluded: false,
    };
    let key = |key, pressed, control, shift| {
        WindowEvent::Input(InputEvent::Key(KeyInput {
            key,
            pressed,
            repeat: false,
            text: None,
            modifiers: zui_api::event::Modifiers {
                control,
                shift,
                ..Default::default()
            },
        }))
    };
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    let root = host.tree().root();
    let first = host.tree().children(root).unwrap()[0];
    let third = host.tree().children(root).unwrap()[2];
    let h1 = host.tree().children(first).unwrap()[0];
    let h3 = host.tree().children(third).unwrap()[0];
    host.update(key(Key::Tab, true, false, false), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(h1));
    for (input, expected) in [
        (Key::ArrowDown, h3),
        (Key::ArrowDown, h1),
        (Key::ArrowUp, h3),
        (Key::Home, h1),
        (Key::End, h3),
        (Key::Home, h1),
    ] {
        host.update(key(input, true, false, false), metrics, Duration::ZERO)
            .unwrap();
        assert_eq!(host.tree().focused(), Some(expected));
    }
    host.update(
        key(Key::ArrowDown, true, true, false),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    host.update(
        key(Key::ArrowDown, false, false, false),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(host.tree().focused(), Some(h1));
    host.update(key(Key::Tab, true, false, false), metrics, Duration::ZERO)
        .unwrap();
    let content = host.tree().focused();
    assert_ne!(content, Some(h1));
    host.update(
        key(Key::ArrowDown, true, false, false),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(host.tree().focused(), content);
    host.update(key(Key::Tab, true, false, true), metrics, Duration::ZERO)
        .unwrap();
    // Moving focus cancels an armed activation, including its subsequent release.
    host.update(key(Key::Space, true, false, false), metrics, Duration::ZERO)
        .unwrap();
    host.update(
        key(Key::ArrowDown, true, false, false),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    host.update(
        key(Key::Space, false, false, false),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(host.tree().focused(), Some(h3));
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(h3));
    host.update(key(Key::End, true, false, false), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(h1));
    host.update(key(Key::Home, true, false, false), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), Some(h3));
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert!(!host.tree().contains(h3));
    assert!(host.tree().focused().is_none());
    host.update(key(Key::Tab, true, false, false), metrics, Duration::ZERO)
        .unwrap();
    host.update(
        key(Key::ArrowDown, true, false, false),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(host.tree().focused(), Some(h1));
}
