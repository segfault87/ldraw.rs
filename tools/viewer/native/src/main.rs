#![cfg(not(target_arch = "wasm32"))]

use std::{
    env,
    rc::Rc,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use async_std::path::PathBuf;
use clap::{App as ClapApp, Arg};
use ldraw::{
    color::ColorCatalog,
    document::MultipartDocument,
    library::{DocumentLoader, LibraryLoader, PartCache},
    resolvers::local::LocalLoader,
};
use viewer_common::App;
use winit::{
    event,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

async fn main_loop<L: LibraryLoader + 'static>(
    document: MultipartDocument,
    colors: ColorCatalog,
    dependency_loader: Rc<L>,
) {
    let evloop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("ldraw.rs demo")
        .build(&evloop)
        .unwrap();

    let mut app = match App::new(window, dependency_loader, Rc::new(colors), true).await {
        Ok(v) => v,
        Err(e) => {
            panic!("Could not initialize app: {e}");
        }
    };
    let cache = Arc::new(RwLock::new(PartCache::new()));
    app.set_document(cache, &document, &|alias, result| {
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

    let started = Instant::now();

    let mut total_duration = 0;
    let mut frames = 0;
    let mut now = started;

    evloop.run(move |event, _, control_flow| match event {
        event::Event::LoopDestroyed => {}
        event::Event::RedrawRequested(_) => {
            app.animate(started.elapsed().as_millis() as f32 / 1000.0);
            match app.render() {
                Ok(duration) => {
                    total_duration += duration.as_millis();
                    frames += 1;

                    if now.elapsed() > Duration::from_secs(1) {
                        println!(
                            "{} frames per second. {} msecs per frame.",
                            frames,
                            total_duration as f32 / frames as f32
                        );

                        now = Instant::now();
                        frames = 0;
                        total_duration = 0;
                    }
                }
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
        event::Event::NewEvents(
            event::StartCause::Init | event::StartCause::ResumeTimeReached { .. },
        ) => {
            app.request_redraw();
        }
        event::Event::MainEventsCleared => {
            app.request_redraw();
        }
        event::Event::WindowEvent { event, .. } => {
            app.handle_window_event(event, started.elapsed().as_millis() as f32 / 1000.0);
        }
        _ => (),
    });
}

#[tokio::main]
async fn main() {
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

    let ldraw_path = PathBuf::from(&ldrawdir);
    let document_base_path = PathBuf::from(&path).parent().map(PathBuf::from);
    let loader = LocalLoader::new(Some(ldraw_path), document_base_path);

    let colors = loader.load_colors().await.unwrap();

    let path_local = PathBuf::from(&path);
    let document = loader.load_document(&path_local, &colors).await.unwrap();

    main_loop(document, colors, Rc::new(loader)).await;
}
