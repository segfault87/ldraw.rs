use std::{
    panic::catch_unwind,
    rc::Rc,
};

use glow::{Context as GlContext, HasContext};
use glutin::{
    dpi::PhysicalSize,
    platform::unix::HeadlessContextExt,
    event_loop::EventLoop,
    ContextBuilder, GlProfile, GlRequest,
};

pub const DEAFULT_WIDTH: usize = 512;
pub const DEFAULT_HEIGHT: usize = 512;

pub fn create_rendering_context<GL: HasContext>(width: usize, height: usize) -> Rc<GlContext> {
    let size_one = PhysicalSize::new(1, 1);
    let cb = ContextBuilder::new()
        .with_gl_profile(GlProfile::Core)
        .with_gl(GlRequest::Latest)
        .with_multisampling(4)
        .with_pixel_format(24, 8);

    let event_loop = match catch_unwind(|| EventLoop::new()) {
        Ok(e) => Some(e),
        Err(_) => None,
    };

    let context = if let Some(ev) = event_loop {
        match cb.clone().build_surfaceless(&ev) {
            Ok(e) => e,
            Err(_) => match cb.clone().build_headless(&ev, size_one) {
                Ok(e) => e,
                Err(_) => match cb.build_osmesa(size_one) {
                    Ok(e) => e,
                    Err(_) => panic!("Could not create rendering context"),
                }
            }
        }
    } else {
        match cb.build_osmesa(size_one) {
            Ok(e) => e,
            Err(_) => panic!("Could not create rendering context"),
        }
    };

    let context = unsafe { context.make_current().unwrap() };

    let gl = unsafe { GlContext::from_loader_function(|s| context.get_proc_address(s) as *const _) };
    let gl = Rc::new(gl);

    unsafe {
        let rb = gl.create_renderbuffer().ok();
        gl.bind_renderbuffer(glow::RENDERBUFFER, rb);
        gl.renderbuffer_storage(
            glow::RENDERBUFFER, glow::RGBA8, width as _, height as _
        );
        let fb = gl.create_framebuffer().ok();
        gl.bind_framebuffer(glow::FRAMEBUFFER, fb);
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, rb);
        gl.viewport(0, 0, width as _, height as _);
    }

    gl
}