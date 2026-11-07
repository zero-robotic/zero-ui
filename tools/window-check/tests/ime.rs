use std::{cell::Cell, rc::Rc, time::Duration};
use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::{View, ViewSpec},
    widget::{Invalidation, PaintContext, TextInputState, Widget},
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::{Painter, Scene};
use zui_platform::{WindowEvent, WindowMetrics};
use zui_types::{Extent, Rect};
#[derive(Clone)]
struct Editor {
    generation: u64,
}
impl ViewSpec for Editor {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, widget: &mut Self) -> Invalidation {
        widget.generation = self.generation;
        Invalidation::Paint
    }
}
impl Widget for Editor {
    fn focusable(&self) -> bool {
        true
    }
    fn text_input(&self, context: PaintContext) -> Option<TextInputState> {
        Some(TextInputState {
            generation: self.generation,
            cursor_area: Rect::new(
                context.content_bounds.origin.x + 10.0,
                context.content_bounds.origin.y,
                1.0,
                20.0,
            ),
        })
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
struct Model {
    generation: u64,
    views: Rc<Cell<u32>>,
}
impl ViewApplication for Model {
    type Message = ();
    fn view(&self) -> View {
        self.views.set(self.views.get() + 1);
        View::new(Editor {
            generation: self.generation,
        })
        .styled(LayoutStyle {
            width: Dimension::Length(200.0),
            height: Dimension::Length(40.0),
            padding: Edges::all(8.0),
            ..Default::default()
        })
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        if event == WindowEvent::Wake {
            self.generation += 1;
            ViewAction::rebuild()
        } else {
            ViewAction::default()
        }
    }
}
fn metrics(scale: f64) -> WindowMetrics {
    WindowMetrics {
        extent: Extent::new((200.0 * scale) as u32, (80.0 * scale) as u32),
        scale,
        occluded: false,
    }
}
fn tab() -> WindowEvent {
    WindowEvent::Input(InputEvent::Key(KeyInput {
        key: Key::Tab,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    }))
}
#[test]
fn host_exports_ime_after_committed_geometry_and_preserves_rebuild_coalescing() {
    let views = Rc::new(Cell::new(0));
    let mut host = WidgetHost::new(
        Model {
            generation: 0,
            views: views.clone(),
        },
        zui_layout::TaffyLayout::default(),
        zui_text::ParleyText::default(),
        Default::default(),
    )
    .unwrap();
    assert!(host.ime_request().is_none());
    host.update(tab(), metrics(1.0), Duration::ZERO).unwrap();
    let original = host.ime_request().unwrap();
    assert_eq!(original.cursor_area, Rect::new(18.0, 8.0, 1.0, 20.0));
    assert_eq!(original.session.owner, host.tree().focused().unwrap().0);
    for _ in 0..3 {
        host.update(WindowEvent::Wake, metrics(1.0), Duration::ZERO)
            .unwrap();
    }
    assert_eq!(views.get(), 1);
    assert_eq!(host.ime_request(), Some(original));
    host.draw(&mut Scene::new(), metrics(2.0), Duration::ZERO)
        .unwrap();
    let updated = host.ime_request().unwrap();
    assert_eq!(views.get(), 2);
    assert_eq!(updated.session.generation, 3);
    assert_eq!(updated.cursor_area, original.cursor_area); // no double DPI scaling
    host.update(WindowEvent::Focused(false), metrics(2.0), Duration::ZERO)
        .unwrap();
    assert!(host.ime_request().is_none());
    host.update(tab(), metrics(1.0), Duration::ZERO).unwrap();
    assert!(host.ime_request().is_some());
    host.update(
        WindowEvent::Occluded(true),
        WindowMetrics {
            occluded: true,
            ..metrics(1.0)
        },
        Duration::ZERO,
    )
    .unwrap();
    assert!(host.ime_request().is_none());
    host.disconnected();
    assert!(host.ime_request().is_none());
}

#[test]
fn text_input_publishes_native_requests_and_rejects_commit_across_pending_external_reset() {
    use std::sync::Arc;
    use zui_api::action::{ActionEvent, ActionId};
    use zui_widgets::{TextInputEvent, TextInputView};
    struct InputModel {
        view: TextInputView,
        changes: usize,
    }
    impl ViewApplication for InputModel {
        type Message = String;
        fn view(&self) -> View {
            View::new(self.view.clone())
                .keyed("editor")
                .styled(LayoutStyle {
                    width: Dimension::Length(200.0),
                    height: Dimension::Length(40.0),
                    padding: Edges::all(8.0),
                    ..Default::default()
                })
        }
        fn map_action(&self, action: &ActionEvent) -> Option<String> {
            match action.action.payload::<TextInputEvent>()? {
                TextInputEvent::Changed(value) => Some(value.clone()),
                _ => None,
            }
        }
        fn message(&mut self, value: String) -> ViewAction {
            self.view.text = value;
            self.changes += 1;
            ViewAction::rebuild()
        }
        fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
            match event {
                WindowEvent::Wake => {
                    self.view.text = "外部".into();
                    self.view.revision += 1;
                    ViewAction::rebuild()
                }
                WindowEvent::Resized => {
                    self.view.read_only = true;
                    ViewAction::rebuild()
                }
                _ => ViewAction::default(),
            }
        }
    }
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let model = InputModel {
        view: TextInputView::themed(&zui_theme::Theme::dark(), "原文", families, ActionId(1)),
        changes: 0,
    };
    let mut host = WidgetHost::new(
        model,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap();
    let metrics = metrics(1.0);
    host.update(tab(), metrics, Duration::ZERO).unwrap();
    let first = host.ime_request().unwrap();
    let commit = |text: &str| WindowEvent::Input(InputEvent::ImeCommit(text.into()));
    host.update(commit("值"), metrics, Duration::ZERO).unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().view.text, "原文值");
    assert_eq!(host.model().changes, 1);
    assert_eq!(host.ime_request().unwrap().session, first.session);
    host.update(WindowEvent::Wake, metrics, Duration::ZERO)
        .unwrap();
    host.update(commit("旧"), metrics, Duration::ZERO).unwrap();
    assert_eq!(host.model().view.text, "外部");
    assert_eq!(host.model().changes, 1);
    assert_ne!(host.ime_request().unwrap().session, first.session);
    host.update(
        WindowEvent::Input(InputEvent::ImePreedit {
            text: "wen".into(),
            cursor: Some((3, 3)),
        }),
        metrics,
        Duration::ZERO,
    )
    .unwrap();
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    host.update(commit("文"), metrics, Duration::ZERO).unwrap();
    assert_eq!(host.model().view.text, "外部文");
    assert_eq!(host.model().changes, 2);
    host.draw(&mut Scene::new(), metrics, Duration::ZERO)
        .unwrap();
    host.update(WindowEvent::Resized, metrics, Duration::ZERO)
        .unwrap();
    host.update(commit("旧"), metrics, Duration::ZERO).unwrap();
    assert_eq!(host.model().view.text, "外部文");
    assert!(host.ime_request().is_none());
}
