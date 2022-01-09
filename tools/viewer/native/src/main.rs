use std::{
    env,
    rc::Rc,
    time::{Duration, Instant},
};

use async_std::path::{Path, PathBuf};
use clap::{Arg, App as ClapApp};
use glow::{self, Context};
use glutin::{
    event::{ElementState, Event, MouseButton, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder, GlProfile, GlRequest,
};
use ldraw::{
    color::MaterialRegistry,
    document::MultipartDocument,
    library::{DocumentLoader, LibraryLoader},
    resolvers::{
        local::LocalLoader,
        http::HttpLoader,
    },
};
use ldraw_renderer::shader::ProgramManager;
use reqwest::Url;
use viewer_common::App;

async fn main_loop(materials: MaterialRegistry, document: MultipartDocument, dependency_loader: Box<dyn LibraryLoader>) {
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

    let mut app = App::new(Rc::clone(&gl), dependency_loader, materials, program_manager);
    app.set_document(&document, &|alias, result| {
        match result {
            Ok(()) => {
                println!("Loaded part {}.", alias);
            }
            Err(e) => {
                println!("Could not load part {}: {}", alias, e);
            }
        };
    })
    .await
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

#[tokio::main]
async fn main() {
    let matches = ClapApp::new("viewer")
        .about("LDraw Model Viewer")
        .arg(Arg::with_name("ldraw_dir")
             .long("ldraw-dir")
             .value_name("PATH_OR_URL")
             .takes_value(true)
             .help("Path or URL to LDraw directory"))
        .arg(Arg::with_name("file")
             .takes_value(true)
             .required(true)
             .value_name("PATH_OR_URL")
             .help("Path or URL to model file"))
        .get_matches();

    let ldrawdir = match matches.value_of("ldraw_dir") {
        Some(v) => v.to_string(),
        None => {
            match env::var("LDRAWDIR") {
                Ok(v) => v,
                Err(_) => {
                    panic!("--ldraw-dir option or LDRAWDIR environment variable is required.");
                }
            }
        }
    };

    let path = String::from(matches.value_of("file").expect("Path is required"));

    // FIXME: There should be better ways than this

    let is_library_remote = ldrawdir.starts_with("http://") || ldrawdir.starts_with("https://");
    let is_document_remote = path.starts_with("http://") || path.starts_with("https://");

    let (ldraw_url, ldraw_path) = if is_library_remote {
        (Url::parse(&ldrawdir).ok(), None)
    } else {
        (None, Some(PathBuf::from(&ldrawdir)))
    };
    
    let (document_base_url, document_base_path) = if is_document_remote {
        let mut url = Url::parse(&path).unwrap();
        url.path_segments_mut().unwrap().pop();
        (Some(url), None)
    } else {
        (None, PathBuf::from(&path).parent().map(|e| PathBuf::from(e)))
    };

    let http_loader = HttpLoader::new(ldraw_url, document_base_url);
    let local_loader = LocalLoader::new(ldraw_path, document_base_path);

    let materials = if is_library_remote {
        http_loader.load_materials().await
    } else {
        local_loader.load_materials().await
    }.unwrap();

    let path_local = PathBuf::from(&path);
    let document = if is_document_remote {
        http_loader.load_document(&materials, &path).await
    } else {
        local_loader.load_document(&materials, &path_local).await
    }.unwrap();

    let loader: Box<dyn LibraryLoader> = if is_library_remote {
        Box::new(http_loader)
    } else {
        Box::new(local_loader)
    };

    main_loop(materials, document, loader).await;
}
