mod app;
mod texture;

use std::{
    env,
    rc::Rc,
    sync::{Arc, RwLock},
};

use async_std::path::PathBuf;
use clap::{App as ClapApp, Arg};
use ldraw::{
    color::ColorCatalog,
    document::MultipartDocument,
    library::{DocumentLoader, LibraryLoader, PartCache},
    resolvers::{http::HttpLoader, local::LocalLoader},
};
use reqwest::Url;
use winit::{
    event,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

async fn main_loop(
    document: MultipartDocument,
    colors: ColorCatalog,
    dependency_loader: Rc<dyn LibraryLoader>,
) {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let mut app = app::App::new(window, dependency_loader, Rc::new(colors)).await;

    let cache = Arc::new(RwLock::new(PartCache::new()));
    app.set_document(cache, &document).await.unwrap();

    event_loop.run(move |event, _, control_flow| match event {
        event::Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == app.window.id() => match event {
            event::WindowEvent::CloseRequested
            | event::WindowEvent::KeyboardInput {
                input:
                    event::KeyboardInput {
                        state: event::ElementState::Pressed,
                        virtual_keycode: Some(event::VirtualKeyCode::Escape),
                        ..
                    },
                ..
            } => *control_flow = ControlFlow::Exit,
            event::WindowEvent::Resized(physical_size) => {
                app.resize(*physical_size);
            }
            event::WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                app.resize(**new_inner_size);
            }
            _ => {}
        },
        event::Event::RedrawRequested(window_id) if window_id == app.window.id() => {
            app.update();
            match app.render() {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    app.resize(app.size);
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    *control_flow = ControlFlow::Exit;
                }
                Err(wgpu::SurfaceError::Timeout) => {
                    println!("Surface timeout");
                }
            }
        }
        event::Event::MainEventsCleared => {
            app.window.request_redraw();
        }
        _ => {}
    });
}

async fn run() {
    let matches = ClapApp::new("viewer")
        .about("LDraw Model Viewer")
        .arg(
            Arg::with_name("ldraw_dir")
                .long("ldraw-dir")
                .value_name("PATH_OR_URL")
                .takes_value(true)
                .help("Path or URL to LDraw directory"),
        )
        .arg(
            Arg::with_name("file")
                .takes_value(true)
                .required(true)
                .value_name("PATH_OR_URL")
                .help("Path or URL to model file"),
        )
        .get_matches();

    let ldrawdir = match matches.value_of("ldraw_dir") {
        Some(v) => v.to_string(),
        None => match env::var("LDRAWDIR") {
            Ok(v) => v,
            Err(_) => {
                panic!("--ldraw-dir option or LDRAWDIR environment variable is required.");
            }
        },
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
        (None, PathBuf::from(&path).parent().map(PathBuf::from))
    };

    let http_loader = HttpLoader::new(ldraw_url, document_base_url);
    let local_loader = LocalLoader::new(ldraw_path, document_base_path);

    let colors = if is_library_remote {
        http_loader.load_colors().await
    } else {
        local_loader.load_colors().await
    }
    .unwrap();

    let path_local = PathBuf::from(&path);
    let document = if is_document_remote {
        http_loader.load_document(&path, &colors).await
    } else {
        local_loader.load_document(&path_local, &colors).await
    }
    .unwrap();

    let loader: Rc<dyn LibraryLoader> = if is_library_remote {
        Rc::new(http_loader)
    } else {
        Rc::new(local_loader)
    };

    main_loop(document, colors, loader).await;
}

fn main() {
    pollster::block_on(run());
}
