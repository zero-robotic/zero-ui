use super::*;
use std::cell::RefCell;
use zui_api::view::View;
use zui_core::Tree;

struct Unused;
impl OpenGlRenderer for Unused {
    fn render(&mut self, _: &glow::Context, _: OpenGlFrame) -> Result<(), OpenGlError> {
        panic!("must not render");
    }
}
fn layout(tree: &mut Tree, size: Size) {
    tree.layout_with_text(
        &mut zui_layout::TaffyLayout::default(),
        &mut zui_text::ParleyText::default(),
        size,
    )
    .unwrap();
}
#[test]
fn validates_dpi_and_allocation_before_native_calls() {
    assert_eq!(
        pixel_extent(Size::new(101.0, 21.0), 1.25).unwrap(),
        Extent::new(127, 27)
    );
    assert_eq!(
        pixel_extent(Size::new(100.0, 20.0), 2.0).unwrap(),
        Extent::new(200, 40)
    );
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            pixel_extent(Size::new(10.0, 10.0), scale),
            Err(OpenGlError::InvalidSize)
        );
    }
    assert_eq!(
        pixel_extent(Size::new(8192.0, 8192.0), 1.0),
        Err(OpenGlError::TooLarge)
    );
    assert_eq!(
        pixel_extent(Size::new(f64::MAX, 1.0), 2.0),
        Err(OpenGlError::TooLarge)
    );
}
#[test]
fn readback_flips_rows_and_converts_alpha() {
    let pixels = vec![0, 0, 255, 255, 200, 100, 50, 128];
    let image =
        renderer::image_from_readback(Extent::new(1, 2), pixels.clone(), OpenGlAlphaMode::Straight)
            .unwrap();
    assert_eq!(image.bytes(), &[100, 50, 25, 128, 0, 0, 255, 255]);
    let image =
        renderer::image_from_readback(Extent::new(1, 2), pixels, OpenGlAlphaMode::Opaque).unwrap();
    assert_eq!(image.bytes(), &[200, 100, 50, 255, 0, 0, 255, 255]);
    assert!(
        renderer::image_from_readback(
            Extent::new(1, 1),
            vec![255, 0, 0, 128],
            OpenGlAlphaMode::Premultiplied
        )
        .is_err()
    );
}
#[test]
fn zero_hidden_and_failed_widgets_do_not_spin_or_allocate() {
    let errors = Rc::new(RefCell::new(Vec::new()));
    let captured = errors.clone();
    let mut view = OpenGlView::new(Size::new(100.0, 60.0), || Unused)
        .on_error(move |e| captured.borrow_mut().push(e.clone()));
    view.scale_factor = f64::NAN;
    view.frame_interval = Some(Duration::from_millis(16));
    view.visible = false;
    let mut tree = Tree::from_view(View::new(view.clone()), Default::default()).unwrap();
    layout(&mut tree, Size::new(100.0, 60.0));
    assert_eq!(
        tree.advance_animations(Duration::ZERO).unwrap().next_frame,
        None
    );
    assert!(errors.borrow().is_empty());
    view.visible = true;
    tree.reconcile(View::new(view.clone())).unwrap();
    layout(&mut tree, Size::new(100.0, 60.0));
    assert_eq!(
        tree.advance_animations(Duration::ZERO).unwrap().next_frame,
        None
    );
    assert_eq!(errors.borrow().as_slice(), &[OpenGlError::InvalidSize]);
    tree.advance_animations(Duration::from_secs(1)).unwrap();
    assert_eq!(errors.borrow().len(), 1);
    view.request_redraw();
    tree.reconcile(View::new(view)).unwrap();
    tree.advance_animations(Duration::from_secs(2)).unwrap();
    assert_eq!(errors.borrow().len(), 2);
    let mut zero = Tree::from_view(
        View::new(OpenGlView::new(Size::ZERO, || Unused)),
        Default::default(),
    )
    .unwrap();
    layout(&mut zero, Size::ZERO);
    assert_eq!(
        zero.advance_animations(Duration::ZERO).unwrap().next_frame,
        None
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_rejects_worker_thread_before_creating_a_context() {
    assert!(
        matches!(native::Native::new(), Err(OpenGlError::Backend(message)) if message.contains("main thread"))
    );
}
