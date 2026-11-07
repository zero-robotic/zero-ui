use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    window::Window,
};
use zui_platform::{NativeWindow, WindowMetrics};
use zui_types::{Extent, Rect, Size};

pub(crate) struct DesktopWindow(pub Window);

impl HasWindowHandle for DesktopWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}
impl HasDisplayHandle for DesktopWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}
impl NativeWindow for DesktopWindow {
    fn metrics(&self) -> WindowMetrics {
        let size = self.0.inner_size();
        WindowMetrics {
            extent: Extent::new(size.width, size.height),
            scale: self.0.scale_factor(),
            occluded: false,
        }
    }
    fn request_redraw(&self) {
        self.0.request_redraw();
    }
    fn request_size(&self, size: Size) {
        if size.is_valid() && size.width > 0.0 && size.height > 0.0 {
            let _ = self
                .0
                .request_inner_size(LogicalSize::new(size.width, size.height));
        }
    }
    fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }
    fn set_ime_enabled(&self, enabled: bool) {
        self.0.set_ime_allowed(enabled);
    }
    fn set_ime_cursor_area(&self, rect: Rect) {
        if rect.is_valid() {
            self.0.set_ime_cursor_area(
                LogicalPosition::new(rect.origin.x, rect.origin.y),
                LogicalSize::new(rect.size.width, rect.size.height),
            );
        }
    }
    fn pre_present_notify(&self) {
        self.0.pre_present_notify();
    }
}
