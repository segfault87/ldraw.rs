use std::{
    env,
    rc::Rc,
    time::{Duration, Instant},
};

use async_std::path::{Path, PathBuf};
use futures::executor::block_on;
use glow::{self, Context};
use glutin::{
    event::{ElementState, Event, MouseButton, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder, GlProfile, GlRequest,
};
use ldraw::{library::FileLoader, resolvers::local::LocalFileLoader};
use ldraw_renderer::shader::ProgramManager;
use viewer_common::App;

fn main_loop(loader: LocalFileLoader, locator: &Path) {
    let evloop = EventLoop::new();
    let window_builder = WindowBuilder::new().with_title("ldraw.rs demo");
    let windowed_context = ContextBuilder::new()
        .with_gl_profile(GlProfile::Core)
        .with_gl(GlRequest::Latest)
        .with_multisampling(4)
        .with_vsync(true)
        .build_windowed(window_builder, &evloop)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };
    let gl = unsafe {
        Context::from_loader_function(|s| windowed_context.get_proc_address(s) as *const _)
    };
    let gl = Rc::new(gl);

    let program_manager = match ProgramManager::new(Rc::clone(&gl)) {
        Ok(e) => e,
        Err(e) => panic!("{}", e),
    };

    let materials = block_on(loader.load_materials()).unwrap();

    let mut app = App::new(Rc::clone(&gl), loader, materials, program_manager);
    block_on(app.set_document(&PathBuf::from(locator), &|alias, result| {
        match result {
            Ok(()) => {
                println!("Loaded part {}.", alias);
            }
            Err(e) => {
                println!("Could not load part {}: {}", alias, e);
            }
        };
    }))
    .unwrap();

    let window = windowed_context.window();
    let size = window.inner_size();
    app.resize(size.width, size.height);

    let started = Instant::now();
    app.set_up();

    let refresh_duration = Duration::from_nanos(16_666_667);

    evloop.run(move |event, _, control_flow| match event {
        Event::LoopDestroyed => {}
        Event::RedrawRequested(_) => {
            app.render();

            windowed_context.swap_buffers().unwrap();
        }
        Event::NewEvents(StartCause::Init) => {
            *control_flow = ControlFlow::WaitUntil(Instant::now() + refresh_duration);
        }
        Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
            app.animate(started.elapsed().as_millis() as f32 / 1000.0);
            app.render();
            windowed_context.swap_buffers().unwrap();
            *control_flow = ControlFlow::WaitUntil(Instant::now() + refresh_duration);
        }
        Event::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => {
                *control_flow = ControlFlow::Exit;
            }
            WindowEvent::Resized(size) => {
                windowed_context.resize(size);
                app.resize(size.width, size.height);
            }
            WindowEvent::KeyboardInput { input, .. } => {
                if input.virtual_keycode == Some(VirtualKeyCode::Space)
                    && input.state == ElementState::Pressed
                {
                    app.advance(started.elapsed().as_millis() as f32 / 1000.0);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    app.orbit.on_mouse_press(state == ElementState::Pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                app.orbit
                    .on_mouse_move(position.x as f32, position.y as f32);
            }
            _ => (),
        },
        _ => (),
    });
}

fn main() {
    let ldrawdir = match env::var("LDRAWDIR") {
        Ok(val) => val,
        Err(e) => panic!("{}", e),
    };
    let ldrawpath = Path::new(&ldrawdir);

    let filedir = match env::args().nth(1) {
        Some(val) => val,
        None => panic!("usage: loader [filename]"),
    };
    let filepath = Path::new(&filedir);

    let file_loader = LocalFileLoader::new(ldrawpath, filepath.parent().unwrap());

    main_loop(file_loader, filepath);
}
