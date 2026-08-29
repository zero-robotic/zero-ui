//! Platform-independent widgets, layout, events, focus and semantics.

pub mod event;
pub mod focus;
pub mod layout;
pub mod layouts;
pub mod semantics;
pub mod theme;
pub mod tree;
pub mod widget;
pub mod widgets;

pub use event::{Action, ActionKind, EventContext, EventResult, SceneUpdate, UiEvent};
pub use focus::FocusManager;
pub use layout::{Constraints, LayoutBox};
pub use layouts::{
    Align, Alignment, ColumnLayout, Gap, GridLayout, Layout, LayoutContext, LayoutStrategy,
    Padding, RowLayout, SizedBox, Spacer, StackLayout, WrapLayout,
};
pub use semantics::{SemanticRole, SemanticsNode};
pub use theme::{
    ButtonStyle, CheckboxStyle, IconButtonStyle, IconStyle, RadioStyle, SwitchStyle,
    TextInputStyle, TextStyle, Theme, ThemeToken,
};
pub use tree::WidgetTree;
pub use widget::{
    PaintContext, RenderBuildContext, Widget, WidgetId, WidgetRuntime, WidgetRuntimeTable,
};
pub use widgets::{
    Button, Checkbox, Divider, DividerAxis, Icon, IconButton, IconName, Radio, Switch, Text,
    TextInput,
};
pub use zui_render::{
    ClipShape, DirtyFlags, DirtyRegionSet, DirtyState, FillRule, FrameStats, IconPath, ImageId,
    ImageResource, PathCommand, RenderNode, RenderNodeBuilder, RenderNodeId, RenderNodeIndex,
    ResourceBudget, ResourceCache, ResourceHandle, ResourceManager, ResourceUsage, Transform,
};

#[cfg(test)]
mod tests {
    use super::*;
    use zui_core::{Dip, Point, Rect, Size};
    use zui_platform::{InputEvent, KeyCode, KeyState, MouseButton};

    #[test]
    fn row_lays_out_children_with_spacing() {
        let mut row = Layout::new(RowLayout::new().spacing(Dip(4.0)))
            .child(Text::new("A"))
            .child(Text::new("BB"));
        let size = row.layout(Constraints::loose(Size {
            width: Dip(200.0),
            height: Dip(100.0),
        }));
        assert_eq!(
            size.width,
            Dip(row.children()[0].bounds().size.width.0
                + 4.0
                + row.children()[1].bounds().size.width.0,)
        );
        assert_eq!(
            row.children()[1].bounds().origin.x,
            Dip(row.children()[0].bounds().size.width.0 + 4.0)
        );
    }

    #[test]
    fn button_emits_clicked_action_for_left_pointer_press() {
        let mut button = Button::new("OK");
        button.set_bounds(Rect {
            origin: Point {
                x: Dip(10.0),
                y: Dip(10.0),
            },
            size: Size {
                width: Dip(60.0),
                height: Dip(30.0),
            },
        });
        let event = UiEvent::pointer(
            None,
            Point {
                x: Dip(20.0),
                y: Dip(20.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let mut context = EventContext::new();
        assert_eq!(
            button.event(&event, &mut context),
            EventResult::RequestRedraw
        );
        assert_eq!(context.actions()[0].kind, ActionKind::Clicked);
    }

    #[test]
    fn checkbox_toggles_and_emits_checked_changed_action() {
        let mut checkbox = Checkbox::new("Enable");
        checkbox.set_bounds(Rect {
            origin: Point {
                x: Dip(10.0),
                y: Dip(10.0),
            },
            size: Size {
                width: Dip(120.0),
                height: Dip(30.0),
            },
        });
        let event = UiEvent::pointer(
            None,
            Point {
                x: Dip(20.0),
                y: Dip(20.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let mut context = EventContext::new();

        assert_eq!(
            checkbox.event(&event, &mut context),
            EventResult::RequestRedraw
        );
        assert!(checkbox.is_checked());
        assert_eq!(context.actions()[0].kind, ActionKind::CheckedChanged);
    }

    #[test]
    fn radio_selects_once_and_emits_selection_changed_action() {
        let mut radio = Radio::new("Normal");
        radio.set_bounds(Rect {
            origin: Point {
                x: Dip(10.0),
                y: Dip(10.0),
            },
            size: Size {
                width: Dip(120.0),
                height: Dip(30.0),
            },
        });
        let event = UiEvent::pointer(
            None,
            Point {
                x: Dip(20.0),
                y: Dip(20.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let mut context = EventContext::new();

        assert_eq!(
            radio.event(&event, &mut context),
            EventResult::RequestRedraw
        );
        assert!(radio.is_selected());
        assert_eq!(context.actions()[0].kind, ActionKind::SelectionChanged);

        let mut second_context = EventContext::new();
        assert_eq!(
            radio.event(&event, &mut second_context),
            EventResult::Handled
        );
        assert!(second_context.actions().is_empty());
    }

    #[test]
    fn text_uses_renderer_measurement_for_multilingual_content() {
        let mut text = Text::new("计数：0");
        let theme = Theme::default();
        text.set_theme(&theme);
        let size = text.measure(Constraints::loose(Size {
            width: Dip(300.0),
            height: Dip(100.0),
        }));

        assert_eq!(
            size.width,
            zui_render::measure_text("计数：0", theme.text.font_size)
        );
    }

    #[test]
    fn switch_toggles_and_emits_checked_changed_action() {
        let mut switch = Switch::new("Automatic");
        switch.set_bounds(Rect {
            origin: Point {
                x: Dip(10.0),
                y: Dip(10.0),
            },
            size: Size {
                width: Dip(140.0),
                height: Dip(30.0),
            },
        });
        let event = UiEvent::pointer(
            None,
            Point {
                x: Dip(20.0),
                y: Dip(20.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let mut context = EventContext::new();

        assert_eq!(
            switch.event(&event, &mut context),
            EventResult::RequestRedraw
        );
        assert!(switch.is_checked());
        assert_eq!(context.actions()[0].kind, ActionKind::CheckedChanged);
    }

    #[test]
    fn text_input_appends_text_when_focused() {
        let mut input = TextInput::new();
        input.set_bounds(Rect {
            origin: Point {
                x: Dip(0.0),
                y: Dip(0.0),
            },
            size: Size {
                width: Dip(200.0),
                height: Dip(32.0),
            },
        });
        let focus = UiEvent::pointer(
            None,
            Point {
                x: Dip(2.0),
                y: Dip(2.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let text = UiEvent::input(InputEvent::Text("hi".into()));
        let mut context = EventContext::new();
        input.event(&focus, &mut context);
        input.event(&text, &mut context);
        assert_eq!(input.text(), "hi");
    }

    #[test]
    fn text_input_accepts_committed_text_events() {
        let mut input = TextInput::new();
        input.set_bounds(Rect {
            origin: Point {
                x: Dip(0.0),
                y: Dip(0.0),
            },
            size: Size {
                width: Dip(200.0),
                height: Dip(32.0),
            },
        });
        let focus = UiEvent::pointer(
            None,
            Point {
                x: Dip(2.0),
                y: Dip(2.0),
            },
            InputEvent::MouseInput {
                button: MouseButton::Left,
                state: KeyState::Pressed,
            },
        );
        let first = UiEvent::input(InputEvent::Text("a".into()));
        let second = UiEvent::input(InputEvent::Text("b".into()));
        let mut context = EventContext::new();
        input.event(&focus, &mut context);
        input.event(&first, &mut context);
        input.event(&second, &mut context);
        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn text_input_replaces_selected_text() {
        let mut input = TextInput::with_text("hello");
        input.set_bounds(Rect {
            origin: Point::default(),
            size: Size {
                width: Dip(200.0),
                height: Dip(32.0),
            },
        });
        input.set_focused(true);
        let select_all = UiEvent::input(InputEvent::Keyboard {
            key: KeyCode::Character('a'),
            state: KeyState::Pressed,
            modifiers: zui_platform::Modifiers {
                control: true,
                ..Default::default()
            },
        });
        let replace = UiEvent::input(InputEvent::Text("x".into()));
        let mut context = EventContext::new();
        input.event(&select_all, &mut context);
        input.event(&replace, &mut context);
        assert_eq!(input.text(), "x");
    }

    #[test]
    fn runtime_focus_transfers_between_text_inputs() {
        let mut first = TextInput::new();
        let mut second = TextInput::new();
        first.set_bounds(Rect {
            origin: Point::default(),
            size: Size {
                width: Dip(100.0),
                height: Dip(30.0),
            },
        });
        second.set_bounds(Rect {
            origin: Point {
                x: Dip(120.0),
                y: Dip::ZERO,
            },
            size: Size {
                width: Dip(100.0),
                height: Dip(30.0),
            },
        });
        let mut runtime = WidgetRuntimeTable::default();
        let press = |point| {
            UiEvent::pointer(
                None,
                point,
                InputEvent::MouseInput {
                    button: MouseButton::Left,
                    state: KeyState::Pressed,
                },
            )
        };
        let mut context = EventContext::with_runtime(&mut runtime);
        first.event(
            &press(Point {
                x: Dip(4.0),
                y: Dip(4.0),
            }),
            &mut context,
        );
        second.event(
            &press(Point {
                x: Dip(124.0),
                y: Dip(4.0),
            }),
            &mut context,
        );

        assert!(!context.is_focused(first.id()));
        assert!(context.is_focused(second.id()));
    }

    #[test]
    fn padding_offsets_child_before_layout() {
        let mut padding = Padding::new(Text::new("x"), Dip(16.0));
        padding.layout(Constraints::loose(Size {
            width: Dip(200.0),
            height: Dip(100.0),
        }));
        assert_eq!(
            padding.child().bounds().origin,
            Point {
                x: Dip(16.0),
                y: Dip(16.0)
            }
        );
    }

    #[test]
    fn sized_box_uses_child_size_when_dimension_is_not_fixed() {
        let mut box_widget = SizedBox::new(Text::new("hello")).width(100.0);
        let size = box_widget.layout(Constraints::loose(Size {
            width: Dip(200.0),
            height: Dip(100.0),
        }));
        assert_eq!(size.width, Dip(100.0));
        assert_eq!(
            size.height,
            Dip((Theme::default().text.font_size * 7) as f32)
        );
    }

    #[test]
    fn align_supports_center_left() {
        let mut align = Align::new(Text::new("x"), Alignment::CENTER_LEFT);
        align.layout(Constraints::tight(Size {
            width: Dip(100.0),
            height: Dip(40.0),
        }));
        assert_eq!(
            align.child().bounds().origin,
            Point {
                x: Dip(0.0),
                y: Dip((40.0 - (Theme::default().text.font_size * 7) as f32) / 2.0),
            }
        );
    }

    #[test]
    fn row_allocates_remaining_space_to_flexible_spacer() {
        let mut row = Layout::new(RowLayout::new())
            .child(Text::new("A"))
            .child(Spacer::flex(1.0))
            .child(Text::new("B"));
        let size = row.layout(Constraints::loose(Size {
            width: Dip(100.0),
            height: Dip(40.0),
        }));
        assert_eq!(size.width, Dip(100.0));
        assert_eq!(
            row.children()[1].bounds().size.width,
            Dip(100.0
                - row.children()[0].bounds().size.width.0
                - row.children()[2].bounds().size.width.0,)
        );
    }

    #[test]
    fn column_allocates_remaining_space_to_flexible_spacer() {
        let mut column = Layout::new(ColumnLayout::new())
            .child(Text::new("A"))
            .child(Spacer::flex(1.0))
            .child(Text::new("B"));
        let size = column.layout(Constraints::loose(Size {
            width: Dip(40.0),
            height: Dip(100.0),
        }));
        assert_eq!(size.height, Dip(100.0));
        assert_eq!(
            column.children()[1].bounds().size.height,
            Dip(100.0
                - column.children()[0].bounds().size.height.0
                - column.children()[2].bounds().size.height.0,)
        );
    }

    #[test]
    fn widget_invalidation_drives_incremental_tree_rebuild() {
        let mut tree = WidgetTree::new(Button::new("OK"));
        tree.layout(Constraints::loose(Size {
            width: Dip(120.0),
            height: Dip(40.0),
        }));
        tree.render_node_cached();
        tree.mark_clean();

        let event = UiEvent::pointer(
            None,
            Point {
                x: Dip(10.0),
                y: Dip(10.0),
            },
            InputEvent::CursorMoved {
                position: Point {
                    x: Dip(10.0),
                    y: Dip(10.0),
                },
            },
        );
        tree.event(&event);
        assert!(tree.needs_redraw());
        let node = tree.render_node_cached();
        assert!(node.is_dirty());
        tree.mark_clean();
        assert!(!tree.render_node_cached().is_dirty());
    }
}
