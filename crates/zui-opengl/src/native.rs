//! Platform contexts own their drawable and never borrow the application's window.
use crate::OpenGlError;

#[cfg(target_os = "macos")]
pub(crate) struct Native {
    context: glutin::api::cgl::context::PossiblyCurrentContext,
    display: glutin::api::cgl::display::Display,
}
#[cfg(target_os = "macos")]
impl Native {
    pub fn new() -> Result<Self, OpenGlError> {
        if objc2::MainThreadMarker::new().is_none() {
            return Err(OpenGlError::Backend(
                "macOS OpenGlView must run on the main thread".into(),
            ));
        }
        use glutin::{
            config::ConfigTemplateBuilder,
            context::{ContextApi, ContextAttributesBuilder, Version},
            prelude::*,
        };
        // AppKit display handles carry no borrowed native pointer. This context
        // is surfaceless; all rendering goes to our own FBO.
        let display = unsafe {
            glutin::api::cgl::display::Display::new(raw_window_handle::RawDisplayHandle::AppKit(
                raw_window_handle::AppKitDisplayHandle::new(),
            ))
        }
        .map_err(OpenGlError::backend)?;
        let template = ConfigTemplateBuilder::new().with_alpha_size(8).build();
        let config = unsafe { display.find_configs(template) }
            .map_err(OpenGlError::backend)?
            .next()
            .ok_or_else(|| OpenGlError::Backend("no OpenGL pixel format".into()))?;
        let attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(None);
        let context = unsafe { display.create_context(&config, &attrs) }
            .map_err(OpenGlError::backend)?
            .make_current_surfaceless()
            .map_err(OpenGlError::backend)?;
        Ok(Self { context, display })
    }
    pub fn make_current(&self) -> Result<(), OpenGlError> {
        self.context
            .make_current_surfaceless()
            .map_err(OpenGlError::backend)
    }
    pub fn release(&self) {
        use glutin::prelude::*;
        let _ = self.context.make_not_current_in_place();
    }
    pub fn address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
        use glutin::prelude::*;
        self.display.get_proc_address(name)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use glutin::{
        config::ConfigTemplateBuilder,
        context::{
            ContextApi, ContextAttributesBuilder, GlProfile, PossiblyCurrentContext, Version,
        },
        display::{Display, DisplayApiPreference},
        prelude::*,
        surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
    };
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
    };
    use std::num::{NonZeroIsize, NonZeroU32};
    use windows_sys::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP},
    };
    struct HiddenWindow(isize);
    impl Drop for HiddenWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    pub(crate) struct Native {
        context: PossiblyCurrentContext,
        surface: Surface<WindowSurface>,
        display: Display,
        // Dropped last: WGL context and surface must not outlive their HWND.
        _window: HiddenWindow,
    }
    impl Native {
        pub fn new() -> Result<Self, OpenGlError> {
            // A private invisible window supplies a DC and WGL extension bootstrap.
            // It is never shown or composited as a child of the ZUI window.
            let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    class.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    0,
                    0,
                    instance,
                    std::ptr::null(),
                )
            };
            let hwnd = NonZeroIsize::new(hwnd)
                .ok_or_else(|| OpenGlError::backend(std::io::Error::last_os_error()))?;
            let window = HiddenWindow(hwnd.get());
            let mut handle = Win32WindowHandle::new(hwnd);
            handle.hinstance = NonZeroIsize::new(instance);
            let raw = RawWindowHandle::Win32(handle);
            let display = unsafe {
                Display::new(
                    RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
                    DisplayApiPreference::Wgl(Some(raw)),
                )
            }
            .map_err(OpenGlError::backend)?;
            let template = ConfigTemplateBuilder::new()
                .with_alpha_size(8)
                .compatible_with_native_window(raw)
                .build();
            let config = unsafe { display.find_configs(template) }
                .map_err(OpenGlError::backend)?
                .next()
                .ok_or_else(|| OpenGlError::Backend("no OpenGL pixel format".into()))?;
            let attrs = ContextAttributesBuilder::new()
                .with_profile(GlProfile::Core)
                .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
                .build(Some(raw));
            let context =
                unsafe { display.create_context(&config, &attrs) }.map_err(OpenGlError::backend)?;
            let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
                raw,
                NonZeroU32::new(1).unwrap(),
                NonZeroU32::new(1).unwrap(),
            );
            let surface = unsafe { display.create_window_surface(&config, &attrs) }
                .map_err(OpenGlError::backend)?;
            let context = context
                .make_current(&surface)
                .map_err(OpenGlError::backend)?;
            Ok(Self {
                context,
                surface,
                display,
                _window: window,
            })
        }
        pub fn make_current(&self) -> Result<(), OpenGlError> {
            self.context
                .make_current(&self.surface)
                .map_err(OpenGlError::backend)
        }
        pub fn release(&self) {
            let _ = self.context.make_not_current_in_place();
        }
        pub fn address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
            self.display.get_proc_address(name)
        }
    }
}
#[cfg(target_os = "windows")]
pub(crate) use windows::Native;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) struct Native;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl Native {
    pub fn new() -> Result<Self, OpenGlError> {
        Err(OpenGlError::Backend(
            "native OpenGL adapter currently supports Windows and macOS".into(),
        ))
    }
    pub fn make_current(&self) -> Result<(), OpenGlError> {
        Self::new().map(|_| ())
    }
    pub fn release(&self) {}
    pub fn address(&self, _: &std::ffi::CStr) -> *const std::ffi::c_void {
        std::ptr::null()
    }
}

/// Unbind even when a user callback unwinds.
pub(crate) struct Current<'a>(&'a Native);
impl<'a> Current<'a> {
    pub fn enter(native: &'a Native) -> Result<Self, OpenGlError> {
        native.make_current()?;
        Ok(Self(native))
    }
}
impl Drop for Current<'_> {
    fn drop(&mut self) {
        self.0.release();
    }
}
