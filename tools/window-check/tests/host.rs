use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};
use zui_api::{
    layout::{Dimension, LayoutStyle},
    text::{TextError, TextLayout, TextRequest, TextService},
    view::View,
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_paint::Scene;
use zui_platform::{WindowEvent, WindowMetrics};
use zui_theme::{Theme, TypeRole};
use zui_types::Extent;
use zui_widgets::TextView;

struct Model {
    families: Vec<String>,
    views: Rc<Cell<usize>>,
    light: bool,
    invalid: bool,
}
impl ViewApplication for Model {
    type Message = ();
    fn view(&self) -> View {
        self.views.set(self.views.get() + 1);
        let theme = if self.light {
            Theme::light()
        } else {
            Theme::dark()
        };
        let families = if self.invalid {
            vec!["missing-family".into()]
        } else {
            self.families.clone()
        };
        View::new(TextView::themed(
            &theme,
            "真实文字排版随窗口宽度变化，Rust 与中文混排。",
            families,
            TypeRole::Body,
        ))
        .keyed("text")
        .styled(LayoutStyle {
            width: Dimension::Percent(1.0),
            ..Default::default()
        })
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        match event {
            WindowEvent::Wake => {
                self.light = !self.light;
                ViewAction::rebuild()
            }
            WindowEvent::Focused(invalid) => {
                self.invalid = invalid;
                ViewAction::rebuild()
            }
            _ => ViewAction::default(),
        }
    }
}
struct Text {
    inner: zui_text::ParleyText,
    calls: Rc<Cell<usize>>,
}
impl TextService for Text {
    fn font_epoch(&self) -> u64 {
        self.inner.font_epoch()
    }
    fn layout(&mut self, request: TextRequest<'_>) -> Result<TextLayout, TextError> {
        self.calls.set(self.calls.get() + 1);
        self.inner.layout(request)
    }
}
struct Fixture {
    host: WidgetHost<Model, zui_layout::TaffyLayout, Text>,
    views: Rc<Cell<usize>>,
    calls: Rc<Cell<usize>>,
}
fn setup() -> Fixture {
    let views = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let mut inner = zui_text::ParleyText::default();
    let families = inner
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let model = Model {
        families,
        views: views.clone(),
        light: false,
        invalid: false,
    };
    let host = WidgetHost::new(
        model,
        Default::default(),
        Text {
            inner,
            calls: calls.clone(),
        },
        Default::default(),
    )
    .unwrap();
    Fixture { host, views, calls }
}
fn metrics(width: u32, scale: f64) -> WindowMetrics {
    WindowMetrics {
        extent: Extent::new(width, 600),
        scale,
        occluded: false,
    }
}
#[test]
fn updates_coalesce_and_palette_or_dpi_changes_do_not_relayout() {
    let Fixture {
        mut host,
        views,
        calls,
    } = setup();
    let mut scene = Scene::new();
    let size = metrics(400, 1.0);
    host.draw(&mut scene, size, Duration::ZERO).unwrap();
    let id = host.tree().root();
    let initial = scene.commands().to_vec();
    let measured = calls.get();
    for _ in 0..3 {
        assert!(
            host.update(WindowEvent::Wake, size, Duration::ZERO)
                .unwrap()
                .redraw
        );
    }
    assert_eq!(views.get(), 1);
    host.draw(&mut scene, size, Duration::ZERO).unwrap();
    assert_eq!(views.get(), 2);
    assert_eq!(host.tree().root(), id);
    assert_eq!(calls.get(), measured);
    assert_ne!(scene.commands(), initial);
    let mut doubled = metrics(800, 2.0);
    doubled.extent.height = 1200;
    assert!(
        host.update(WindowEvent::ScaleChanged, doubled, Duration::ZERO)
            .unwrap()
            .redraw
    );
    host.draw(&mut scene, doubled, Duration::ZERO).unwrap();
    assert_eq!(calls.get(), measured);
    let height = host.tree().layout_box(id).unwrap().bounds.size.height;
    let narrow = metrics(150, 1.0);
    assert!(
        host.update(WindowEvent::Resized, narrow, Duration::ZERO)
            .unwrap()
            .redraw
    );
    host.draw(&mut scene, narrow, Duration::ZERO).unwrap();
    assert!(calls.get() > measured);
    assert!(host.tree().layout_box(id).unwrap().bounds.size.height > height);
}
#[test]
fn failed_draw_preserves_caller_scene_and_recovers_from_new_state() {
    let Fixture { mut host, .. } = setup();
    let mut scene = Scene::new();
    let size = metrics(400, 1.0);
    host.draw(&mut scene, size, Duration::ZERO).unwrap();
    let initial = scene.commands().to_vec();
    host.update(WindowEvent::Focused(true), size, Duration::ZERO)
        .unwrap();
    let input_error = host
        .update(
            WindowEvent::Input(zui_platform::InputEvent::PointerMoved(
                zui_types::Point::new(10.0, 10.0),
            )),
            size,
            Duration::ZERO,
        )
        .unwrap_err();
    assert!(input_error.to_string().contains("UnknownFamily"));
    let error = host.draw(&mut scene, size, Duration::ZERO).unwrap_err();
    assert!(error.to_string().contains("UnknownFamily"));
    assert_eq!(scene.commands(), initial);
    assert!(host.tree().layout_box(host.tree().root()).is_err());
    host.update(WindowEvent::Focused(false), size, Duration::ZERO)
        .unwrap();
    host.draw(&mut scene, size, Duration::ZERO).unwrap();
    assert_eq!(scene.commands(), initial);
}
#[test]
fn hidden_frames_defer_pending_view_work_until_drawable() {
    let Fixture {
        mut host,
        views,
        calls,
    } = setup();
    let mut scene = Scene::new();
    let mut size = metrics(400, 1.0);
    host.update(WindowEvent::Wake, size, Duration::ZERO)
        .unwrap();
    size.occluded = true;
    host.draw(&mut scene, size, Duration::ZERO).unwrap();
    assert_eq!(views.get(), 1);
    assert_eq!(calls.get(), 0);
    size.occluded = false;
    host.draw(&mut scene, size, Duration::ZERO).unwrap();
    assert_eq!(views.get(), 2);
    assert!(!scene.commands().is_empty());
}
