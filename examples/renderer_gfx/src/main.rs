use gfx_backend_gl as backend;
use gfx_hal::{
    format::{AsFormat, Rgba8Srgb as ColorFormat},
    window,
};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event_loop::EventLoop,
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new();
    let wb = WindowBuilder::new()
        .with_min_inner_size(LogicalSize::new(1.0, 1.0))
        .with_inner_size(LogicalSize::from_physical(
            PhysicalSize::new(1024.0, 768.0),
            event_loop.primary_monitor().hidpi_factor()
        ))
        .with_title("renderer-gfx demo");
    let window = wb.build(&event_loop).unwrap();
    let builder = backend::config_context(
        backend::glutin::ContextBuilder::new(), ColorFormat::SELF, None
    );
    let windowed_context = builder.build_windowed(wb, &event_loop).unwrap();
    let (context, window) = unsafe {
        windowed_context
            .make_current()
            .expect("Unable to make context current")
            .split()
    };
    let surface = backend::Surface::from_context(context);

    let mut adapters = surface.enumerate_adapters();
}
