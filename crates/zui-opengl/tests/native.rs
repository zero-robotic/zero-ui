use std::{cell::Cell, rc::Rc, time::Duration};
use zui_api::{event::*, view::View, widget::Invalidation};
use zui_core::Tree;
use zui_opengl::*;
use zui_types::{Color, Extent, Size};
fn layout(tree: &mut Tree, size: Size) {
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut zui_text::ParleyText::default(),
        size,
    )
    .unwrap();
}
#[derive(Default)]
struct Counts {
    init: Cell<u32>,
    resize: Cell<u32>,
    render: Cell<u32>,
    destroy: Cell<u32>,
    input: Cell<u32>,
}
struct Pattern(Rc<Counts>);
impl OpenGlRenderer for Pattern {
    fn initialize(&mut self, _: &glow::Context) -> Result<(), OpenGlError> {
        self.0.init.set(self.0.init.get() + 1);
        Ok(())
    }
    fn resize(&mut self, _: &glow::Context, _: Extent, _: f64) -> Result<(), OpenGlError> {
        self.0.resize.set(self.0.resize.get() + 1);
        Ok(())
    }
    fn render(&mut self, gl: &glow::Context, f: OpenGlFrame) -> Result<(), OpenGlError> {
        use glow::HasContext;
        self.0.render.set(self.0.render.get() + 1);
        unsafe {
            gl.clear_color(0.0, 0.0, 1.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                0,
                f.extent.height as i32 / 2,
                f.extent.width as i32,
                f.extent.height as i32 / 2,
            );
            gl.clear_color(1.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            // Framework must undo client pack state before readback.
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 1024);
            gl.pixel_store_i32(glow::PACK_SKIP_ROWS, 8);
        }
        Ok(())
    }
    fn input(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        if matches!(
            e,
            WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
        ) {
            assert_eq!(c.local_position, Some(zui_types::Point::new(5.0, 5.0)));
            self.0.input.set(self.0.input.get() + 1);
            return EventResponse {
                handled: true,
                invalidation: Invalidation::Paint,
                ..Default::default()
            };
        }
        EventResponse::default()
    }
    fn destroy(&mut self, _: &glow::Context) {
        self.0.destroy.set(self.0.destroy.get() + 1);
    }
}
fn main() {
    if !std::env::args().any(|arg| arg == "--native") {
        println!("native OpenGL test skipped; pass --native to run on the main thread");
        return;
    }
    use zui_paint::{Frame, OffscreenRenderer};
    let counts = Rc::new(Counts::default());
    let captured = counts.clone();
    let mut view = OpenGlView::new(Size::new(40.0, 20.0), move || Pattern(captured.clone()))
        .on_error(|e| panic!("{e}"));
    view.scale_factor = 2.0;
    let mut tree = Tree::from_view(View::new(view.clone()), Default::default()).unwrap();
    layout(&mut tree, Size::new(40.0, 20.0));
    let animation = tree.advance_animations(Duration::ZERO).unwrap();
    assert_eq!(animation.next_frame, None);
    assert_eq!(counts.init.get(), 1);
    assert_eq!(counts.render.get(), 1);
    let pixels = zui_render_cpu::CpuRenderer::default()
        .render(
            &tree.scene().unwrap(),
            Frame {
                extent: Extent::new(80, 40),
                scale: 2.0,
                background: Color::BLACK,
            },
        )
        .unwrap();
    for (y, expected) in [(5, [255, 0, 0, 255]), (35, [0, 0, 255, 255])] {
        let i = y * pixels.stride() + 20 * 4;
        assert_eq!(&pixels.pixels()[i..i + 4], &expected);
    }
    tree.advance_animations(Duration::from_secs(1)).unwrap();
    tree.scene().unwrap();
    assert_eq!(counts.render.get(), 1);
    tree.dispatch_input(InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed: true,
        position: Some(zui_types::Point::new(5.0, 5.0)),
    })
    .unwrap();
    tree.advance_animations(Duration::from_secs(2)).unwrap();
    assert_eq!(counts.input.get(), 1);
    assert_eq!(counts.render.get(), 2);
    view.scale_factor = 1.25;
    view.frame_interval = Some(Duration::from_millis(16));
    tree.reconcile(View::new(view.clone())).unwrap();
    let animation = tree.advance_animations(Duration::from_secs(3)).unwrap();
    assert_eq!(animation.next_frame, Some(Duration::from_millis(3016)));
    assert_eq!(counts.resize.get(), 2);
    assert_eq!(counts.init.get(), 1);
    tree.advance_animations(Duration::from_millis(3010))
        .unwrap();
    assert_eq!(counts.render.get(), 3);
    tree.advance_animations(Duration::from_millis(3016))
        .unwrap();
    assert_eq!(counts.render.get(), 4);
    view.visible = false;
    tree.reconcile(View::new(view.clone())).unwrap();
    assert_eq!(
        tree.advance_animations(Duration::from_secs(4))
            .unwrap()
            .next_frame,
        None
    );
    assert_eq!(counts.render.get(), 4);
    // Interleave a second independent context, then resume the first one.
    let other_counts = Rc::new(Counts::default());
    let capture = other_counts.clone();
    let mut other = Tree::from_view(
        View::new(
            OpenGlView::new(Size::new(20.0, 20.0), move || Pattern(capture.clone()))
                .on_error(|e| panic!("{e}")),
        ),
        Default::default(),
    )
    .unwrap();
    layout(&mut other, Size::new(20.0, 20.0));
    other.advance_animations(Duration::ZERO).unwrap();
    assert_eq!(other_counts.init.get(), 1);
    view.visible = true;
    tree.reconcile(View::new(view)).unwrap();
    tree.advance_animations(Duration::from_secs(5)).unwrap();
    assert_eq!(counts.render.get(), 5);
    drop(other);
    assert_eq!(other_counts.destroy.get(), 1);
    drop(tree);
    assert_eq!(counts.destroy.get(), 1);
    failed_initialization_cleans_up_and_can_retry();
    println!(
        "native OpenGL checks passed: pixels, input, DPI, scheduling, independent contexts and failure cleanup"
    );
}

struct Failing {
    fail: Rc<Cell<bool>>,
    destroys: Rc<Cell<u32>>,
    buffer: Option<glow::Buffer>,
}
impl OpenGlRenderer for Failing {
    fn initialize(&mut self, gl: &glow::Context) -> Result<(), OpenGlError> {
        use glow::HasContext;
        self.buffer = Some(unsafe { gl.create_buffer() }.map_err(OpenGlError::Backend)?);
        if self.fail.get() {
            Err(OpenGlError::Backend(
                "intentional partial initialization failure".into(),
            ))
        } else {
            Ok(())
        }
    }
    fn render(&mut self, _: &glow::Context, _: OpenGlFrame) -> Result<(), OpenGlError> {
        Ok(())
    }
    fn destroy(&mut self, gl: &glow::Context) {
        use glow::HasContext;
        if let Some(buffer) = self.buffer.take() {
            unsafe {
                gl.delete_buffer(buffer);
            }
        }
        self.destroys.set(self.destroys.get() + 1);
    }
}
fn failed_initialization_cleans_up_and_can_retry() {
    let fail = Rc::new(Cell::new(true));
    let destroys = Rc::new(Cell::new(0));
    let errors = Rc::new(Cell::new(0));
    let f = fail.clone();
    let d = destroys.clone();
    let e = errors.clone();
    let mut view = OpenGlView::new(Size::new(10.0, 10.0), move || Failing {
        fail: f.clone(),
        destroys: d.clone(),
        buffer: None,
    })
    .on_error(move |error| {
        assert!(error.to_string().contains("intentional"));
        e.set(e.get() + 1);
    });
    let mut tree = Tree::from_view(View::new(view.clone()), Default::default()).unwrap();
    layout(&mut tree, Size::new(10.0, 10.0));
    tree.advance_animations(Duration::ZERO).unwrap();
    assert_eq!(destroys.get(), 1);
    assert_eq!(errors.get(), 1);
    tree.advance_animations(Duration::from_secs(1)).unwrap();
    assert_eq!(errors.get(), 1);
    fail.set(false);
    view.request_redraw();
    tree.reconcile(View::new(view)).unwrap();
    tree.advance_animations(Duration::from_secs(2)).unwrap();
    assert_eq!(errors.get(), 1);
    drop(tree);
    assert_eq!(destroys.get(), 2);
}
