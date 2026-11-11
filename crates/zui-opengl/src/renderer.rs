use crate::{
    OpenGlAlphaMode, OpenGlError, OpenGlFrame, OpenGlRenderer,
    native::{Current, Native},
};
use glow::HasContext;
use zui_paint::{ImageLimits, ImageResource};
use zui_types::Extent;

struct Target {
    fbo: glow::Framebuffer,
    color: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    extent: Extent,
}
impl Target {
    // All callers hold this session's current context.
    unsafe fn new(gl: &glow::Context, extent: Extent) -> Result<Self, OpenGlError> {
        unsafe {
            let limit = gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE).max(0) as u32;
            if extent.width > limit || extent.height > limit {
                return Err(OpenGlError::TooLarge);
            }
            let fbo = gl.create_framebuffer().map_err(OpenGlError::Backend)?;
            let color = match gl.create_renderbuffer() {
                Ok(v) => v,
                Err(e) => {
                    gl.delete_framebuffer(fbo);
                    return Err(OpenGlError::Backend(e));
                }
            };
            let depth = match gl.create_renderbuffer() {
                Ok(v) => v,
                Err(e) => {
                    gl.delete_renderbuffer(color);
                    gl.delete_framebuffer(fbo);
                    return Err(OpenGlError::Backend(e));
                }
            };
            let target = Self {
                fbo,
                color,
                depth,
                extent,
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
            gl.renderbuffer_storage(
                glow::RENDERBUFFER,
                glow::RGBA8,
                extent.width as i32,
                extent.height as i32,
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(color),
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage(
                glow::RENDERBUFFER,
                glow::DEPTH24_STENCIL8,
                extent.width as i32,
                extent.height as i32,
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_STENCIL_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE
                || gl.get_error() != glow::NO_ERROR
            {
                target.delete(gl);
                return Err(OpenGlError::Backend(
                    "could not allocate complete OpenGL framebuffer".into(),
                ));
            }
            Ok(target)
        }
    }
    unsafe fn delete(self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_renderbuffer(self.color);
            gl.delete_renderbuffer(self.depth);
        }
    }
}

pub(crate) struct Session {
    gl: glow::Context,
    native: Native,
    target: Option<Target>,
    renderer: Box<dyn OpenGlRenderer>,
    initialized: bool,
}
impl Session {
    pub fn new(renderer: Box<dyn OpenGlRenderer>) -> Result<Self, OpenGlError> {
        let native = Native::new()?;
        // Native::new creates and makes current a private desktop GL context.
        let gl = unsafe { glow::Context::from_loader_function_cstr(|name| native.address(name)) };
        native.release();
        let mut session = Self {
            gl,
            native,
            target: None,
            renderer,
            initialized: false,
        };
        {
            let _current = Current::enter(&session.native)?;
            let version = session.gl.version();
            if version.is_embedded || (version.major, version.minor) < (3, 3) {
                return Err(OpenGlError::Backend(
                    "OpenGlView requires desktop OpenGL 3.3 or newer".into(),
                ));
            }
            // Call destroy after a partially failed initialization too.
            session.initialized = true;
            session.renderer.initialize(&session.gl)?;
        }
        Ok(session)
    }
    pub fn input(
        &mut self,
        context: zui_api::event::EventContext,
        event: &zui_api::event::WidgetEvent,
    ) -> zui_api::event::EventResponse {
        self.renderer.input(context, event)
    }
    pub fn render(
        &mut self,
        extent: Extent,
        logical_size: zui_types::Size,
        scale_factor: f64,
        elapsed: std::time::Duration,
        alpha: OpenGlAlphaMode,
    ) -> Result<ImageResource, OpenGlError> {
        let _current = Current::enter(&self.native)?;
        let gl = &self.gl;
        // The callback may keep its own program/VAO/textures between frames.
        // Restore only framework-owned state before accessing the target.
        unsafe {
            if self.target.as_ref().is_none_or(|t| t.extent != extent) {
                let target = Target::new(gl, extent)?;
                if let Some(old) = self.target.replace(target) {
                    old.delete(gl);
                }
                self.renderer.resize(gl, extent, scale_factor)?;
            }
            let target = self.target.as_ref().unwrap();
            let frame = OpenGlFrame {
                framebuffer: target.fbo,
                extent,
                logical_size,
                scale_factor,
                elapsed,
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
            gl.draw_buffer(glow::COLOR_ATTACHMENT0);
            gl.viewport(0, 0, extent.width as i32, extent.height as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.stencil_mask(u32::MAX);
            gl.clear_color(
                0.0,
                0.0,
                0.0,
                if alpha == OpenGlAlphaMode::Opaque {
                    1.0
                } else {
                    0.0
                },
            );
            gl.clear_depth_f32(1.0);
            gl.clear_stencil(0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
            self.renderer.render(gl, frame)?;
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.fbo));
            gl.read_buffer(glow::COLOR_ATTACHMENT0);
            // A user PBO or non-default pack stride would corrupt CPU readback.
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            for (parameter, value) in [
                (glow::PACK_ALIGNMENT, 1),
                (glow::PACK_ROW_LENGTH, 0),
                (glow::PACK_SKIP_ROWS, 0),
                (glow::PACK_SKIP_PIXELS, 0),
                (glow::PACK_SWAP_BYTES, 0),
                (glow::PACK_LSB_FIRST, 0),
            ] {
                gl.pixel_store_i32(parameter, value);
            }
            let mut bytes = vec![0; extent.rgba_len().ok_or(OpenGlError::TooLarge)?];
            gl.read_pixels(
                0,
                0,
                extent.width as i32,
                extent.height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut bytes)),
            );
            let error = gl.get_error();
            if error != glow::NO_ERROR {
                return Err(OpenGlError::Backend(format!(
                    "OpenGL render/readback error: 0x{error:04x}"
                )));
            }
            image_from_readback(extent, bytes, alpha)
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(_current) = Current::enter(&self.native) {
            if self.initialized {
                self.renderer.destroy(&self.gl);
            }
            if let Some(target) = self.target.take() {
                unsafe {
                    target.delete(&self.gl);
                }
            }
        }
        // If the context cannot be made current, context destruction releases
        // driver resources without invoking unsafe GL operations on another context.
    }
}

pub(crate) fn image_from_readback(
    extent: Extent,
    mut bytes: Vec<u8>,
    alpha: OpenGlAlphaMode,
) -> Result<ImageResource, OpenGlError> {
    if extent.width == 0 || extent.height == 0 || extent.rgba_len() != Some(bytes.len()) {
        return Err(OpenGlError::InvalidSize);
    }
    let stride = extent.width as usize * 4;
    for y in 0..extent.height as usize / 2 {
        let opposite = (extent.height as usize - 1 - y) * stride;
        let (top, bottom) = bytes.split_at_mut(opposite);
        top[y * stride..(y + 1) * stride].swap_with_slice(&mut bottom[..stride]);
    }
    match alpha {
        OpenGlAlphaMode::Opaque => {
            for p in bytes.as_chunks_mut::<4>().0 {
                p[3] = 255;
            }
            ImageResource::from_premultiplied_rgba8(extent, bytes.into(), ImageLimits::default())
        }
        OpenGlAlphaMode::Straight => {
            ImageResource::from_rgba8(extent, bytes, ImageLimits::default())
        }
        OpenGlAlphaMode::Premultiplied => {
            ImageResource::from_premultiplied_rgba8(extent, bytes.into(), ImageLimits::default())
        }
    }
    .map_err(OpenGlError::backend)
}
