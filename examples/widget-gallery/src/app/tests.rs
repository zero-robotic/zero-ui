use super::*;
use std::sync::Arc;

mod gallery {
    use super::*;
    use zui_api::action::WidgetAction;
    use zui_widgets::*;
    #[test]
    fn all_gallery_states_layout_and_actions_round_trip() {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut model = Demo::new(families, None);
        let mut tree = zui_core::Tree::from_view(model.view(), Default::default()).unwrap();
        let actions = [
            WidgetAction::new(ActionId(300), ButtonActivated),
            WidgetAction::new(ActionId(300), ButtonActivated),
            WidgetAction::new(ActionId(312), NotificationDismissed { id: 2 }),
            WidgetAction::new(ActionId(100), SelectEvent::Open),
            WidgetAction::new(ActionId(100), SelectEvent::Selected(ListItemId(75))),
            WidgetAction::new(ActionId(101), NumberChanged { value: 35.0 }),
            WidgetAction::new(ActionId(103), ButtonActivated),
            WidgetAction::new(ActionId(104), DialogResult::Accept),
            WidgetAction::new(ActionId(102), MenuEvent::Open(MenuId(0))),
            WidgetAction::new(ActionId(102), MenuEvent::OpenPath(vec![MenuId(3)])),
            WidgetAction::new(
                ActionId(105),
                PopupDismissed {
                    reason: DismissReason::Escape,
                },
            ),
            WidgetAction::new(ActionId(102), MenuEvent::Selected(MenuId(1))),
            WidgetAction::new(ActionId(4), SliderChanged { value: 75.0 }),
            WidgetAction::new(ActionId(6), StepperChanged { value: 80.0 }),
            WidgetAction::new(ActionId(5), TextInputEvent::Changed("同步内容".into())),
            WidgetAction::new(ActionId(10), ToggleChanged { selected: false }),
            WidgetAction::new(ActionId(11), SwitchChanged { checked: true }),
            WidgetAction::new(ActionId(12), DisclosureChanged { expanded: false }),
            WidgetAction::new(
                ActionId(13),
                SegmentChanged {
                    selected: SegmentId(2),
                },
            ),
            WidgetAction::new(
                ActionId(14),
                ListSelectionChanged {
                    selected: ListItemId(3),
                },
            ),
            WidgetAction::new(ActionId(15), TabChanged { selected: TabId(1) }),
            WidgetAction::new(
                ActionId(16),
                AccordionChanged {
                    item: AccordionId(2),
                    expansion: AccordionExpansion::Single(Some(AccordionId(2))),
                },
            ),
            WidgetAction::new(ActionId(1), ButtonActivated),
        ];
        for action in actions {
            let message = model
                .map_action(&ActionEvent {
                    source: tree.root(),
                    action,
                })
                .expect("public action maps to app message");
            assert!(model.message(message).rebuild);
            tree.reconcile(model.view()).unwrap();
            for size in [Size::new(1100.0, 800.0), Size::new(480.0, 640.0)] {
                tree.layout_with_text(&mut zui_layout::TaffyLayout::default(), &mut text, size)
                    .unwrap();
                tree.scene().unwrap().validate(Default::default()).unwrap();
            }
        }
        assert_eq!(model.level, 80.0);
        assert_eq!(model.input, "同步内容");
        assert!(model.light);
    }
}

mod overlay_host_tests {
    use super::*;
    use zui_app::Application;
    use zui_platform::{KeyInput, Modifiers, PointerButton};
    #[test]
    fn pointer_and_keyboard_drive_select_dialog_number_and_submenu_round_trips() {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut host = WidgetHost::new(
            Demo::new(families, None),
            zui_layout::TaffyLayout::default(),
            text,
            Default::default(),
        )
        .unwrap();
        let metrics = WindowMetrics {
            extent: zui_types::Extent::new(1100, 800),
            scale: 1.0,
            occluded: false,
        };
        let mut scene = zui_paint::Scene::new();
        host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
        let send = |host: &mut WidgetHost<Demo, zui_layout::TaffyLayout, zui_text::ParleyText>,
                    input| {
            host.update(WindowEvent::Input(input), metrics, Duration::ZERO)
                .unwrap();
            host.draw(&mut zui_paint::Scene::new(), metrics, Duration::ZERO)
                .unwrap();
        };
        let click =
            |host: &mut WidgetHost<Demo, zui_layout::TaffyLayout, zui_text::ParleyText>, x, y| {
                let point = zui_types::Point::new(x, y);
                send(host, InputEvent::PointerMoved(point));
                for pressed in [true, false] {
                    send(
                        host,
                        InputEvent::PointerButton {
                            button: PointerButton::Primary,
                            pressed,
                            position: Some(point),
                        },
                    );
                }
            };
        let key = |key, text, modifiers| {
            InputEvent::Key(KeyInput {
                key,
                text,
                modifiers,
                pressed: true,
                repeat: false,
            })
        };
        click(&mut host, 40.0, 40.0);
        let popup_focus = host.tree().focused();
        send(&mut host, key(Key::ArrowDown, None, Modifiers::default()));
        assert_eq!(host.model().level, 50.0);
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 75.0);
        assert_ne!(host.tree().focused(), popup_focus);
        click(&mut host, 450.0, 40.0);
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 50.0);
        click(&mut host, 620.0, 40.0);
        send(
            &mut host,
            key(
                Key::Character("a".into()),
                None,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            ),
        );
        send(
            &mut host,
            key(
                Key::Character("60".into()),
                Some("60".into()),
                Modifiers::default(),
            ),
        );
        assert_eq!(host.model().level, 50.0);
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 60.0);
        click(&mut host, 240.0, 40.0);
        send(&mut host, key(Key::End, None, Modifiers::default()));
        send(&mut host, key(Key::ArrowRight, None, Modifiers::default()));
        send(&mut host, key(Key::Enter, None, Modifiers::default()));
        assert_eq!(host.model().level, 100.0);
        click(&mut host, 450.0, 40.0);
        let shortcut = || {
            key(
                Key::Character("R".into()),
                None,
                Modifiers {
                    control: true,
                    shift: true,
                    ..Default::default()
                },
            )
        };
        send(&mut host, shortcut());
        assert_eq!(
            host.model().level,
            100.0,
            "modal blocks background shortcuts"
        );
        send(&mut host, key(Key::Escape, None, Modifiers::default()));
        send(&mut host, shortcut());
        assert_eq!(host.model().level, 25.0);
    }
}

mod anchor_tests {
    use super::*;
    use zui_app::Application;
    use zui_types::{NodeId, Point};
    fn find(tree: &zui_core::Tree, key: &str) -> NodeId {
        let mut pending = vec![tree.root()];
        while let Some(id) = pending.pop() {
            if tree.key(id).unwrap() == Some(&zui_api::view::Key::from(key)) {
                return id;
            }
            pending.extend_from_slice(tree.children(id).unwrap());
        }
        panic!("missing {key}");
    }
    #[test]
    fn date_popup_uses_the_activated_start_or_end_button_bounds() {
        use zui_api::action::WidgetAction;
        use zui_widgets::{Date, DateEvent, DatePickerView};
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        for action in [302, 303] {
            let mut demo = Demo::new(families.clone(), None);
            let theme = demo.theme();
            let trigger = DatePickerView::themed(
                &theme,
                Date::new(2026, 11, 1).unwrap(),
                families.clone(),
                ActionId(action),
            )
            .build()
            .unwrap()
            .keyed("date-trigger");
            let view = column([trigger])
                .style(|s| {
                    s.padding = Edges::all(40.0);
                    s.width = Dimension::Length(240.0);
                })
                .build();
            let mut tree = zui_core::Tree::from_view(view, Default::default()).unwrap();
            tree.layout_with_text(
                &mut zui_layout::TaffyLayout::default(),
                &mut text,
                Size::new(800.0, 800.0),
            )
            .unwrap();
            let source = find(&tree, "date-trigger");
            let button = tree.presented_bounds(source).unwrap();
            let message = demo
                .map_action_with_tree(
                    &ActionEvent {
                        source,
                        action: WidgetAction::new(ActionId(action), DateEvent::Open),
                    },
                    &tree,
                )
                .unwrap();
            demo.message(message);
            let overlay = demo.catalog.overlay(&theme, &families).unwrap();
            let mut popup = zui_core::Tree::from_view(overlay, Default::default()).unwrap();
            popup
                .layout_with_text(
                    &mut zui_layout::TaffyLayout::default(),
                    &mut text,
                    Size::new(800.0, 800.0),
                )
                .unwrap();
            let bounds = popup
                .presented_bounds(popup.children(popup.root()).unwrap()[0])
                .unwrap();
            assert!((bounds.origin.x - button.origin.x).abs() < 0.01);
            assert!((bounds.origin.y - button.origin.y - button.size.height).abs() < 0.01);
            demo.catalog.apply(catalog::Message::CloseDate);
            assert!(demo.catalog.overlay(&theme, &families).is_none());
        }
    }
    #[test]
    fn dropdowns_anchor_to_actual_buttons_after_layout_and_scroll() {
        for (width, scale) in [(1100, 1.0), (1280, 2.0)] {
            let mut text = zui_text::ParleyText::default();
            let families = text
                .register_font(Arc::from(
                    include_bytes!("../../../../assets/fonts/NotoSansSC.ttf").as_slice(),
                ))
                .unwrap();
            let mut host = WidgetHost::new(
                Demo::new(families, None),
                zui_layout::TaffyLayout::default(),
                text,
                Default::default(),
            )
            .unwrap();
            let metrics = WindowMetrics {
                extent: zui_types::Extent::new(width, (1000.0 * scale) as u32),
                scale,
                occluded: false,
            };
            let mut scene = zui_paint::Scene::new();
            host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
            for (button_key, overlay_key) in [
                ("preset-select", "popup"),
                ("device-combo", "advanced-overlay"),
            ] {
                let id = find(host.tree(), button_key);
                if button_key == "device-combo" {
                    let before = host.tree().presented_bounds(id).unwrap();
                    host.update(
                        WindowEvent::Input(InputEvent::PointerMoved(Point::new(
                            before.origin.x + 5.0,
                            before.origin.y + 5.0,
                        ))),
                        metrics,
                        Duration::ZERO,
                    )
                    .unwrap();
                    host.update(
                        WindowEvent::Input(InputEvent::Scroll(zui_platform::ScrollDelta::Pixels {
                            x: 0.0,
                            y: -10.0,
                        })),
                        metrics,
                        Duration::ZERO,
                    )
                    .unwrap();
                    host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
                    assert!(host.tree().presented_bounds(id).unwrap().origin.y < before.origin.y);
                }
                let button = host.tree().presented_bounds(id).unwrap();
                let point = Point::new(button.origin.x + 5.0, button.origin.y + 5.0);
                for pressed in [true, false] {
                    host.update(
                        WindowEvent::Input(InputEvent::PointerButton {
                            button: zui_platform::PointerButton::Primary,
                            pressed,
                            position: Some(point),
                        }),
                        metrics,
                        Duration::ZERO,
                    )
                    .unwrap();
                }
                host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
                let overlay = find(host.tree(), overlay_key);
                let content = host.tree().children(overlay).unwrap()[0];
                let popup = host.tree().presented_bounds(content).unwrap();
                assert!(
                    (popup.origin.x - button.origin.x).abs() < 0.01,
                    "{button_key}: {button:?} vs {popup:?}"
                );
                assert!(
                    (popup.origin.y - button.origin.y - button.size.height).abs() < 0.01,
                    "{button_key}: {button:?} vs {popup:?}"
                );
                assert!((popup.size.width - button.size.width).abs() < 0.01);
                let action = host
                    .update(WindowEvent::Resized, metrics, Duration::ZERO)
                    .unwrap();
                assert!(action.redraw);
                host.draw(&mut scene, metrics, Duration::ZERO).unwrap();
                assert!(!host.tree().contains(overlay));
            }
        }
    }
}
