use std::{sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    event::{InputEvent, Key, KeyInput},
    layout::{Dimension, LayoutStyle},
    view::View,
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::Scene;
use zui_platform::{WindowEvent, WindowMetrics};
use zui_theme::Theme;
use zui_types::Extent;
use zui_widgets::{SegmentChanged, SegmentId, SegmentOption, SegmentedControlView};
struct Model {
    families: Vec<String>,
    selected: SegmentId,
    changes: usize,
    disabled: bool,
}
impl ViewApplication for Model {
    type Message = SegmentId;
    fn view(&self) -> View {
        let mut disabled = SegmentOption::new(SegmentId(2), "禁用");
        disabled.disabled = true;
        let mut control = SegmentedControlView::themed(
            &Theme::dark(),
            vec![
                SegmentOption::new(SegmentId(1), "概览"),
                disabled,
                SegmentOption::new(SegmentId(3), "趋势"),
            ],
            self.families.clone(),
            Some(self.selected),
            ActionId(12),
        );
        control.disabled = self.disabled;
        View::new(control).styled(LayoutStyle {
            width: Dimension::Length(320.0),
            ..Default::default()
        })
    }
    fn map_action(&self, action: &ActionEvent) -> Option<SegmentId> {
        if action.action.id != ActionId(12) {
            return None;
        }
        action
            .action
            .payload::<SegmentChanged>()
            .map(|event| event.selected)
    }
    fn message(&mut self, selected: SegmentId) -> ViewAction {
        self.selected = selected;
        self.changes += 1;
        ViewAction::rebuild()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.disabled = true;
            ViewAction::rebuild()
        } else {
            ViewAction::default()
        }
    }
}
#[test]
fn segmented_selection_round_trips_through_model_and_reconciles_without_changing_focus_identity() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let mut host = WidgetHost::new(
        Model {
            families,
            selected: SegmentId(1),
            changes: 0,
            disabled: false,
        },
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap();
    let metrics = WindowMetrics {
        extent: Extent::new(320, 80),
        scale: 1.0,
        occluded: false,
    };
    let input = |key, pressed| {
        WindowEvent::Input(InputEvent::Key(KeyInput {
            key,
            pressed,
            repeat: false,
            text: None,
            modifiers: Default::default(),
        }))
    };
    host.update(input(Key::Tab, true), metrics, Duration::ZERO)
        .unwrap();
    let focused = host.tree().focused();
    host.update(input(Key::ArrowRight, true), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().selected, SegmentId(3));
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.tree().focused(), focused);
    host.update(input(Key::Space, true), metrics, Duration::ZERO)
        .unwrap();
    host.update(input(Key::Space, false), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().changes, 1);
    host.update(input(Key::ArrowLeft, true), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().selected, SegmentId(1));
    assert_eq!(host.model().changes, 2);
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert!(host.tree().focused().is_none());
}
