use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    window::{ImeCapabilities, ImeEnableRequest, ImeRequest, ImeRequestData, Window},
};
use zui_platform::{NativeWindow, WindowMetrics};
use zui_types::{Extent, Rect, Size};

pub(crate) struct DesktopWindow(pub Box<dyn Window>, pub winit::event_loop::EventLoopProxy);
struct DesktopWaker(winit::event_loop::EventLoopProxy);
impl zui_platform::EventWaker for DesktopWaker {
    fn wake(&self) -> Result<(), zui_platform::PlatformError> {
        self.0.wake_up();
        Ok(())
    }
}

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
        let size = self.0.surface_size();
        WindowMetrics {
            extent: Extent::new(size.width, size.height),
            scale: self.0.scale_factor(),
            occluded: false,
        }
    }
    fn event_waker(&self) -> Option<std::sync::Arc<dyn zui_platform::EventWaker>> {
        Some(std::sync::Arc::new(DesktopWaker(self.1.clone())))
    }
    fn request_redraw(&self) {
        self.0.request_redraw();
    }
    fn request_size(&self, size: Size) {
        if size.is_valid() && size.width > 0.0 && size.height > 0.0 {
            let _ = self
                .0
                .request_surface_size(LogicalSize::new(size.width, size.height).into());
        }
    }
    fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }
    fn drag_window(&self) -> Result<(), zui_platform::PlatformError> {
        self.0
            .drag_window()
            .map_err(|e| zui_platform::PlatformError::Backend(e.to_string()))
    }
    fn set_minimized(&self, minimized: bool) -> Result<(), zui_platform::PlatformError> {
        self.0.set_minimized(minimized);
        Ok(())
    }
    fn set_maximized(&self, maximized: bool) -> Result<(), zui_platform::PlatformError> {
        self.0.set_maximized(maximized);
        Ok(())
    }
    fn set_ime_enabled(&self, enabled: bool) {
        let request = if enabled {
            let data = ImeRequestData::default().with_cursor_area(
                LogicalPosition::new(0.0, 0.0).into(),
                LogicalSize::new(0.0, 0.0).into(),
            );
            ImeRequest::Enable(
                ImeEnableRequest::new(ImeCapabilities::new().with_cursor_area(), data)
                    .expect("cursor-area capability has initial data"),
            )
        } else {
            ImeRequest::Disable
        };
        let _ = self.0.request_ime_update(request);
    }
    fn set_ime_cursor_area(&self, rect: Rect) {
        if rect.is_valid() {
            let data = ImeRequestData::default().with_cursor_area(
                LogicalPosition::new(rect.origin.x, rect.origin.y).into(),
                LogicalSize::new(rect.size.width, rect.size.height).into(),
            );
            let _ = self.0.request_ime_update(ImeRequest::Update(data));
        }
    }
    fn pre_present_notify(&self) {
        self.0.pre_present_notify();
    }
}
