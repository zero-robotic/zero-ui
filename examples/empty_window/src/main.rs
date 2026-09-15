use zui_app::Application;
use zui_backend_winit::WinitBackend;
use zui_render_runtime::ActiveRenderer;
use zui_ui::Spacer;

fn empty_root() -> Spacer {
    // The root intentionally emits no paint commands. The renderer must still
    // attach the surface and clear every frame to the application background.
    Spacer::flex(1.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = WinitBackend::new()?;
    let renderer = ActiveRenderer::new_blocking()?;
    Application::new()
        .title("zero-ui empty window")
        .run_with(backend, renderer, empty_root())
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_backend_headless::HeadlessBackend;
    use zui_render_runtime::HeadlessRenderer;

    #[test]
    fn empty_window_attaches_and_clears_a_default_surface() {
        let renderer = Application::new()
            .run_with(
                HeadlessBackend::new(),
                HeadlessRenderer::new(),
                empty_root(),
            )
            .expect("the empty-window composition root must submit a frame");
        let frame = renderer
            .last_frame()
            .expect("a clear frame must be present");

        assert_eq!(frame.metrics.physical_size.width, 800);
        assert_eq!(frame.metrics.physical_size.height, 600);
        assert!(frame.node.commands.is_empty());
        assert!(frame.node.children.is_empty());
    }
}
