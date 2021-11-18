use std::{
    panic::catch_unwind,
    rc::Rc,
};

use glow::{
    Context as GlContext, HasContext
};
use glutin::{
    dpi::PhysicalSize,
    platform::unix::HeadlessContextExt,
    event_loop::EventLoop,
    Context, ContextBuilder, CreationError, GlProfile, GlRequest,
    NotCurrent, PossiblyCurrent
};
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
    
    gl_context: Context<PossiblyCurrent>,

    framebuffer: Option<glow::NativeFramebuffer>,
    renderbuffer_color: Option<glow::NativeRenderbuffer>,
    renderbuffer_depth: Option<glow::NativeRenderbuffer>,
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
        framebuffer = gl.create_framebuffer().ok();
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);

        renderbuffer_depth = gl.create_renderbuffer().ok();
        gl.bind_renderbuffer(glow::RENDERBUFFER, renderbuffer_depth);
        gl.renderbuffer_storage(
            glow::RENDERBUFFER, glow::DEPTH_COMPONENT32F, width as _, height as _
        );
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, renderbuffer_depth);

        renderbuffer_color = gl.create_renderbuffer().ok();
        gl.bind_renderbuffer(glow::RENDERBUFFER, renderbuffer_color);
        gl.renderbuffer_storage(
            glow::RENDERBUFFER, glow::RGBA8, width as _, height as _
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

        gl_context: context,

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
