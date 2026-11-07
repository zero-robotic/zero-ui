use std::{cell::Cell, rc::Rc, time::Duration};
use zui_api::{
    event::{EventContext, EventResponse, InputEvent, WidgetEvent},
    layout::{Dimension, LayoutStyle},
    view::{Key, View, ViewSpec},
    widget::{Invalidation, PaintContext, TextInputState, Widget},
};
use zui_app::{Application, ViewAction, ViewApplication, ViewFocusRequest, WidgetHost};
use zui_core::{Tree, TreeError};
use zui_paint::{Painter, Scene};
use zui_platform::{WindowAction, WindowEvent, WindowMetrics};
use zui_types::{Extent, NodeId, Rect};
use zui_widgets::{ScrollAlignment, VirtualListView};

#[derive(Clone)]
struct Editor(Rc<Cell<usize>>);
impl ViewSpec for Editor {
    type Widget = Self;
    fn build(&self) -> Self {
        self.clone()
    }
    fn update(&self, _: &mut Self) -> Invalidation {
        Invalidation::None
    }
}
impl Widget for Editor {
    fn focusable(&self) -> bool {
        true
    }
    fn text_input(&self, _: PaintContext) -> Option<TextInputState> {
        Some(TextInputState {
            generation: 0,
            cursor_area: Rect::new(2.0, 2.0, 1.0, 12.0),
        })
    }
    fn event(&mut self, _: EventContext, event: &WidgetEvent) -> EventResponse {
        match event {
            WidgetEvent::Input(InputEvent::ImeCommit(_)) => self.0.set(self.0.get() + 1),
            // Ensure Host completes geometry invalidated by focus notification.
            WidgetEvent::FocusChanged(_) => {
                return EventResponse {
                    invalidation: Invalidation::Layout,
                    ..Default::default()
                };
            }
            _ => {}
        }
        EventResponse::default()
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
struct Model {
    index: usize,
    pending: bool,
    clear: bool,
    invalid: bool,
    commits: Rc<Cell<usize>>,
    requests: usize,
}
impl ViewApplication for Model {
    type Message = ();
    fn view(&self) -> View {
        let commits = self.commits.clone();
        VirtualListView::new(1000, 20.0, move |index| {
            View::new(Editor(commits.clone()))
                .keyed(index as u64)
                .styled(LayoutStyle {
                    height: Dimension::Length(20.0),
                    ..Default::default()
                })
        })
        .overscan(0)
        .scroll_to_index(self.index, ScrollAlignment::Start, self.index as u64)
        .style(|s| {
            s.width = Dimension::Length(100.0);
            s.height = Dimension::Length(60.0);
        })
        .build()
    }
    fn take_focus_request(&mut self, tree: &Tree) -> Option<ViewFocusRequest> {
        if !self.pending {
            return None;
        }
        assert_ne!(tree.invalidation(), Invalidation::Layout);
        let request = if self.invalid {
            ViewFocusRequest::Focus(NodeId(u64::MAX))
        } else if self.clear {
            ViewFocusRequest::Clear
        } else {
            let slot = tree
                .children(tree.root())
                .unwrap()
                .iter()
                .copied()
                .find(|id| tree.key(*id).unwrap() == Some(&Key::Number(self.index as u64)))?;
            ViewFocusRequest::Focus(tree.children(slot).unwrap()[0])
        };
        self.pending = false;
        self.requests += 1;
        Some(request)
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        match event {
            WindowEvent::Wake => {
                self.index = 900;
                self.pending = true;
                ViewAction::rebuild()
            }
            WindowEvent::Resized => {
                self.clear = true;
                self.pending = true;
                ViewAction {
                    window: WindowAction::redraw(),
                    rebuild: false,
                }
            }
            WindowEvent::ScaleChanged => {
                self.invalid = true;
                self.pending = true;
                ViewAction::rebuild()
            }
            _ => ViewAction::default(),
        }
    }
}
fn host() -> WidgetHost<Model, zui_layout::TaffyLayout, zui_text::ParleyText> {
    WidgetHost::new(
        Model {
            index: 0,
            pending: true,
            clear: false,
            invalid: false,
            commits: Rc::new(Cell::new(0)),
            requests: 0,
        },
        zui_layout::TaffyLayout::default(),
        zui_text::ParleyText::default(),
        Default::default(),
    )
    .unwrap()
}
fn metrics() -> WindowMetrics {
    WindowMetrics {
        extent: Extent::new(100, 60),
        scale: 1.0,
        occluded: false,
    }
}
#[test]
fn host_realizes_then_focuses_and_rejects_ime_from_previous_owner() {
    let mut host = host();
    host.draw(&mut Scene::new(), metrics(), Duration::ZERO)
        .unwrap();
    let original = host.ime_request().unwrap();
    assert_eq!(host.model().requests, 1);
    host.update(WindowEvent::Wake, metrics(), Duration::ZERO)
        .unwrap();
    let commit = || WindowEvent::Input(InputEvent::ImeCommit("old".into()));
    host.update(commit(), metrics(), Duration::ZERO).unwrap();
    assert_eq!(host.model().commits.get(), 0);
    let current = host.ime_request().unwrap();
    assert_ne!(current.session.owner, original.session.owner);
    assert_eq!(host.tree().focused().unwrap().0, current.session.owner);
    assert_eq!(current.cursor_area, Rect::new(2.0, 2.0, 1.0, 12.0));
    host.update(commit(), metrics(), Duration::ZERO).unwrap();
    assert_eq!(host.model().commits.get(), 1);
    host.draw(&mut Scene::new(), metrics(), Duration::ZERO)
        .unwrap();
    assert_eq!(host.model().requests, 2);
    host.update(WindowEvent::Resized, metrics(), Duration::ZERO)
        .unwrap();
    assert!(host.animate(metrics(), Duration::ZERO).unwrap());
    assert!(host.tree().focused().is_none());
    assert!(host.ime_request().is_none());
}
#[test]
fn failed_focus_preparation_clears_ime_and_can_recover_without_replaying_request() {
    let mut host = host();
    host.draw(&mut Scene::new(), metrics(), Duration::ZERO)
        .unwrap();
    let original = host.tree().focused();
    host.update(WindowEvent::ScaleChanged, metrics(), Duration::ZERO)
        .unwrap();
    let mut scene = Scene::new();
    let error = host
        .draw(&mut scene, metrics(), Duration::ZERO)
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<TreeError>(),
        Some(TreeError::MissingNode(_))
    ));
    assert_eq!(host.tree().focused(), original);
    assert!(host.ime_request().is_none());
    host.draw(&mut scene, metrics(), Duration::ZERO).unwrap();
    assert!(host.ime_request().is_some());
    assert_eq!(host.model().requests, 2);
}
