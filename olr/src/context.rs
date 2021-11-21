use std::rc::Rc;

use glow::{
    Context as GlContext, HasContext, PixelPackData,
};
use glutin::{
    dpi::PhysicalSize,
    platform::unix::HeadlessContextExt,
    event_loop::EventLoop,
    Context, ContextBuilder, CreationError, GlProfile, GlRequest,
    NotCurrent, PossiblyCurrent
};
use ldraw::Vector2;
use ldraw_ir::geometry::BoundingBox2;
use image::RgbaImage;
use ldraw_renderer::{
    shader::ProgramManager,
    state::RenderingContext
};

use crate::error::ContextCreationError;

pub struct OlrContext {
    pub width: usize,
    pub height: usize,

    pub gl: Rc<GlContext>,
    pub rendering_context: RenderingContext<GlContext>,
    
    _gl_context: Context<PossiblyCurrent>,

    framebuffer: Option<glow::NativeFramebuffer>,
    renderbuffer_color: Option<glow::NativeRenderbuffer>,
    renderbuffer_depth: Option<glow::NativeRenderbuffer>,
}

impl OlrContext {

    pub fn get_framebuffer_contents(&self, bounds: Option<BoundingBox2>) -> RgbaImage {
        let mut pixels: Vec<u8> = Vec::new();
        pixels.resize(4 * self.width * self.height, 0);

        let gl = &self.gl;
        unsafe {
            // Transfer from multisampled fbo to normal fbo
            let framebuffer_wo_multisample = gl.create_framebuffer().ok();
            gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer_wo_multisample);
            let renderbuffer_color = gl.create_renderbuffer().ok();
            gl.bind_renderbuffer(glow::RENDERBUFFER, renderbuffer_color);
            gl.renderbuffer_storage(
                glow::RENDERBUFFER, glow::RGBA8, self.width as _, self.height as _
            );
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, renderbuffer_color);

            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, self.framebuffer);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, framebuffer_wo_multisample);
            gl.blit_framebuffer(
                0, 0, self.width as _, self.height as _,
                0, 0, self.width as _, self.height as _,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST
            );
            
            gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer_wo_multisample);
            gl.read_buffer(glow::COLOR_ATTACHMENT0);
            gl.read_pixels(
                0, 0, self.width as _, self.height as _, glow::RGBA, glow::UNSIGNED_BYTE,
                PixelPackData::Slice(pixels.as_mut())
            );

            gl.delete_renderbuffer(renderbuffer_color.unwrap());
            gl.delete_framebuffer(framebuffer_wo_multisample.unwrap());
        }

        let bounds = bounds.unwrap_or_else(|| BoundingBox2::new(&Vector2::new(0.0, 0.0), &Vector2::new(1.0, 1.0)));

        let x1 = (bounds.min.x * self.width as f32) as usize;
        let y1 = (bounds.min.y * self.height as f32) as usize;
        let x2 = (bounds.max.x * self.width as f32) as usize;
        let y2 = (bounds.max.y * self.height as f32) as usize;
        let cw = x2 - x1;
        let ch = y2 - y1;

        let mut pixels_rearranged: Vec<u8> = Vec::new();
        for v in (y1..y2).rev() {
            let s = 4 * v as usize * self.width as usize + x1 * 4;
            pixels_rearranged.extend_from_slice(&pixels[s..(s + (cw * 4))]);
        }

        unsafe {
            // Revert back to previous state
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, self.framebuffer);
        }

        RgbaImage::from_raw(cw as _, ch as _, pixels_rearranged).unwrap()
    }

}

impl Drop for OlrContext {
    fn drop(&mut self) {
        let gl = &self.gl;

        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            if let Some(e) = self.renderbuffer_color {
                gl.delete_renderbuffer(e);
            }
            if let Some(e) = self.renderbuffer_depth {
                gl.delete_renderbuffer(e);
            }
            if let Some(e) = self.framebuffer {
                gl.delete_framebuffer(e);
            }
        }
    }
}

fn create_context(
    context: Context<NotCurrent>, width: usize, height: usize
) -> Result<OlrContext, ContextCreationError> {
    let context = unsafe { context.make_current().unwrap() };

    let gl = unsafe { GlContext::from_loader_function(|s| context.get_proc_address(s) as *const _) };
    let gl = Rc::new(gl);

    let framebuffer;
    let renderbuffer_depth;
    let renderbuffer_color;
    unsafe {
        gl.enable(glow::MULTISAMPLE);

        framebuffer = gl.create_framebuffer().ok();
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);

        renderbuffer_depth = gl.create_renderbuffer().ok();
        gl.bind_renderbuffer(glow::RENDERBUFFER, renderbuffer_depth);
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER, 4, glow::DEPTH_COMPONENT32F, width as _, height as _
        );
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, renderbuffer_depth);

        renderbuffer_color = gl.create_renderbuffer().ok();
        gl.bind_renderbuffer(glow::RENDERBUFFER, renderbuffer_color);
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER, 4, glow::RGBA8, width as _, height as _
        );
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, renderbuffer_color);
    }

    let program_manager = ProgramManager::new(Rc::clone(&gl))?;
    let rendering_context = RenderingContext::new(Rc::clone(&gl), program_manager);

    Ok(OlrContext {
        width,
        height,

        gl,
        rendering_context,

        _gl_context: context,

        framebuffer,
        renderbuffer_color,
        renderbuffer_depth
    })
}

pub fn create_headless_context<T: 'static>(
    ev: EventLoop<T>, width: usize, height: usize
) -> Result<OlrContext, ContextCreationError> {
    let size = PhysicalSize::new(1, 1);
    let cb = ContextBuilder::new()
        .with_gl_profile(GlProfile::Core)
        .with_gl(GlRequest::Latest)
        .with_pixel_format(24, 8);

    let context = match cb.clone().build_surfaceless(&ev) {
        Ok(e) => e,
        Err(_) => match cb.clone().build_headless(&ev, size) {
            Ok(e) => e,
            Err(e) => {
                if cfg!(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly", target_os = "netbsd", target_os = "openbsd")) {
                    cb.build_osmesa(size)?
                } else {
                    return Err(ContextCreationError::GlContextError(e))
                }
            }
        }
    };

    create_context(context, width, height)
}

pub fn create_osmesa_context(
    width: usize, height: usize
) -> Result<OlrContext, ContextCreationError> {
    if cfg!(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly", target_os = "netbsd", target_os = "openbsd")) {
        let size = PhysicalSize::new(1, 1);
        let cb = ContextBuilder::new()
            .with_gl_profile(GlProfile::Core)
            .with_gl(GlRequest::Latest)
            .with_pixel_format(24, 8);

        let context = cb.build_osmesa(size)?;

        create_context(context, width, height)
    } else {
        Err(ContextCreationError::GlContextError(CreationError::OsError(String::from("Osmesa context is only available for *nix systems."))))
    }
}
