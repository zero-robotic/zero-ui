use std::sync::Arc;
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutKind, LayoutStyle},
    view::View,
};
use zui_core::Tree;
use zui_paint::Command;
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Point, Size};
use zui_widgets::*;

const RING: Color = Color::rgb(255, 0, 255);

fn description(kind: &str, families: Vec<String>) -> View {
    let theme = Theme::dark();
    let view = match kind {
        "button" => {
            let mut view = ButtonView::themed(&theme, "Button", families, ActionId(1));
            view.style.focus_ring = RING;
            View::new(view)
        }
        "checkbox" => {
            let mut view =
                CheckBoxView::themed(&theme, "Check", families, CheckState::Checked, ActionId(1));
            view.style.focus_ring = RING;
            View::new(view)
        }
        "switch" => {
            let mut view = SwitchView::themed(&theme, true, ActionId(1));
            view.style.focus_ring = RING;
            View::new(view)
        }
        "slider" => {
            let mut view = SliderView::themed(
                &theme,
                NumericRange::new(0.0, 100.0, 1.0).unwrap(),
                50.0,
                ActionId(1),
            );
            view.style.focus_ring = RING;
            View::new(view)
        }
        "radio" => {
            let mut view = RadioGroupView::themed(
                &theme,
                vec![RadioOption::new(RadioOptionId(1), "Radio")],
                families,
                Some(RadioOptionId(1)),
                ActionId(1),
            );
            view.style.focus_ring = RING;
            View::new(view)
        }
        "segmented" => {
            let mut view = SegmentedControlView::themed(
                &theme,
                vec![SegmentOption::new(SegmentId(1), "Segment")],
                families,
                Some(SegmentId(1)),
                ActionId(1),
            );
            view.style.focus_ring = RING;
            View::new(view)
        }
        "list" => {
            let mut view = ListBoxView::themed(
                &theme,
                vec![ListItem::new(ListItemId(1), "Item")],
                families,
                Some(ListItemId(1)),
                ActionId(1),
            );
            view.style.focus_ring = RING;
            View::new(view)
        }
        "scroll" => ScrollView::vertical(View::new(TextView::themed(
            &theme,
            "Content",
            families,
            TypeRole::Body,
        )))
        .focus_color(RING)
        .build(),
        "text_input" => {
            let mut view = TextInputView::themed(&theme, "Edit", families, ActionId(1));
            view.style.focus_border = RING;
            View::new(view)
        }
        _ => unreachable!(),
    };
    view.keyed("control").styled(LayoutStyle {
        kind: if kind == "scroll" {
            LayoutKind::Column
        } else {
            LayoutKind::Leaf
        },
        width: Dimension::Length(220.0),
        height: Dimension::Length(48.0),
        padding: Edges::all(8.0),
        ..Default::default()
    })
}

fn key(key: Key) -> InputEvent {
    InputEvent::Key(KeyInput {
        key,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    })
}

fn click(tree: &mut Tree, point: Point) {
    for pressed in [true, false] {
        tree.dispatch_input(InputEvent::PointerButton {
            button: PointerButton::Primary,
            pressed,
            position: Some(point),
        })
        .unwrap();
    }
}

fn has_ring(tree: &mut Tree) -> bool {
    tree.scene()
        .unwrap()
        .commands()
        .iter()
        .any(|command| matches!(command, Command::Stroke {color, ..} if *color == RING))
}

#[test]
fn mouse_focus_is_quiet_and_keyboard_focus_survives_hover_and_reconciliation() {
    for kind in [
        "button",
        "checkbox",
        "switch",
        "slider",
        "radio",
        "segmented",
        "list",
        "scroll",
    ] {
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let mut tree =
            Tree::from_view(description(kind, families.clone()), Default::default()).unwrap();
        let mut engine = zui_layout::TaffyLayout::default();
        tree.layout_with_text(&mut engine, &mut text, Size::new(240.0, 80.0))
            .unwrap();
        let point = if kind == "scroll" {
            Point::new(2.0, 2.0)
        } else {
            Point::new(16.0, 16.0)
        };
        assert!(!has_ring(&mut tree), "{kind}: initial");
        click(&mut tree, point);
        assert_eq!(tree.focused(), Some(tree.root()), "{kind}: owns focus");
        assert!(!has_ring(&mut tree), "{kind}: pointer");
        tree.reconcile(description(kind, families.clone())).unwrap();
        tree.layout_with_text(&mut engine, &mut text, Size::new(240.0, 80.0))
            .unwrap();
        assert!(!has_ring(&mut tree), "{kind}: reconciled pointer");
        tree.dispatch_input(key(Key::Tab)).unwrap();
        assert!(has_ring(&mut tree), "{kind}: keyboard");
        tree.dispatch_input(InputEvent::PointerMoved(point))
            .unwrap();
        assert!(has_ring(&mut tree), "{kind}: hover");
        tree.dispatch_input(InputEvent::ModifiersChanged(Modifiers {
            shift: true,
            ..Default::default()
        }))
        .unwrap();
        assert!(has_ring(&mut tree), "{kind}: modifiers");
        let keyboard_scene = tree.scene().unwrap();
        click(&mut tree, point);
        assert_eq!(tree.focused(), Some(tree.root()));
        assert!(!has_ring(&mut tree), "{kind}: second pointer");
        assert!(
            !keyboard_scene.same_commands(&tree.scene().unwrap()),
            "{kind}: invalidated paint cache"
        );
        tree.dispatch_input(key(Key::ArrowRight)).unwrap();
        assert!(has_ring(&mut tree), "{kind}: keyboard after pointer");
        tree.reconcile(description(kind, families)).unwrap();
        tree.layout_with_text(&mut engine, &mut text, Size::new(240.0, 80.0))
            .unwrap();
        assert!(has_ring(&mut tree), "{kind}: reconciled keyboard");
        tree.request_focus(None).unwrap();
        assert!(!has_ring(&mut tree), "{kind}: blur");
    }
}

#[test]
fn mouse_focused_text_input_keeps_editing_border_and_ime_target() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut tree =
        Tree::from_view(description("text_input", families), Default::default()).unwrap();
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut text,
        Size::new(240.0, 80.0),
    )
    .unwrap();
    assert!(!has_ring(&mut tree));
    click(&mut tree, Point::new(16.0, 16.0));
    assert!(has_ring(&mut tree));
    assert!(tree.text_input().unwrap().is_some());
    tree.request_focus(None).unwrap();
    assert!(!has_ring(&mut tree));
    assert!(tree.text_input().unwrap().is_none());
}

#[test]
fn hsv_picker_keeps_color_markers_but_only_keyboard_adds_focus_outline() {
    let mut tree = Tree::from_view(
        View::new(HsvPickerView {
            hsv: HsvColor::new(120.0, 1.0, 1.0).unwrap(),
            alpha: 255,
            disabled: false,
            action: ActionId(1),
        })
        .styled(LayoutStyle {
            width: Dimension::Length(220.0),
            height: Dimension::Length(180.0),
            ..Default::default()
        }),
        Default::default(),
    )
    .unwrap();
    tree.layout(
        &mut zui_layout::TaffyLayout::default(),
        Size::new(240.0, 200.0),
    )
    .unwrap();
    let strokes = |tree: &mut Tree| {
        tree.scene()
            .unwrap()
            .commands()
            .iter()
            .filter(|command| matches!(command, Command::Stroke { .. }))
            .count()
    };
    let markers = strokes(&mut tree);
    assert!(markers > 0);
    click(&mut tree, Point::new(16.0, 16.0));
    assert_eq!(tree.focused(), Some(tree.root()));
    assert_eq!(strokes(&mut tree), markers);
    tree.dispatch_input(key(Key::Tab)).unwrap();
    assert_eq!(strokes(&mut tree), markers + 1);
    click(&mut tree, Point::new(16.0, 16.0));
    assert_eq!(strokes(&mut tree), markers);
}
