use std::{
    rc::Rc,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use ldraw::{
    color::ColorCatalog,
    document::MultipartDocument,
    library::{DocumentLoader, LibraryLoader, PartCache},
    resolvers::http::HttpLoader,
};
use reqwest::Url;
use tokio::runtime::{Handle, Runtime};
use viewer_common::App;
use winit::{
    application::ApplicationHandler, event, event_loop::EventLoop, platform::ios::WindowExtIOS,
    window::Window,
};

#[no_mangle]
pub extern "C" fn start_viewer_app() {
    let rt = Runtime::new().unwrap();
    let (document, colors, loader) = rt.block_on(load_deps());

    main_loop(rt, document, colors, loader);
}

async fn load_deps() -> (MultipartDocument, Rc<ColorCatalog>, Rc<HttpLoader>) {
    let loader = HttpLoader::new(
        Some(Url::parse("https://segfault87.github.io/ldraw-rs-preview/ldraw/").unwrap()),
        None,
    );

    let colors = loader.load_colors().await.unwrap();
    let document = loader
        .load_document(
            &"https://segfault87.github.io/ldraw-rs-preview/models/6973.ldr".to_owned(),
            &colors,
        )
        .await
        .unwrap();

    (document, Rc::new(colors), Rc::new(loader))
}

struct AppOuter {
    rt: Runtime,
    app: Option<App<HttpLoader>>,
    document: MultipartDocument,
    loader: Rc<HttpLoader>,
    colors: Rc<ColorCatalog>,
    started: Instant,
    now: Instant,
    total_duration: i32,
    frames: i32,
}

impl AppOuter {
    fn new(
        rt: Runtime,
        document: MultipartDocument,
        loader: Rc<HttpLoader>,
        colors: Rc<ColorCatalog>,
    ) -> Self {
        let now = Instant::now();
        Self {
            rt,
            app: None,
            document,
            loader,
            colors,
            started: now,
            now,
            total_duration: 0,
            frames: 0,
        }
    }
}

impl ApplicationHandler for AppOuter {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_transparent(true)
            .with_maximized(true)
            .with_active(true)
            .with_visible(true);

        let window = event_loop.create_window(window_attrs).unwrap();

        window.recognize_pinch_gesture(true);

        let mut app = self
            .rt
            .block_on(App::new(
                Arc::new(window),
                Rc::clone(&self.loader),
                Rc::clone(&self.colors),
                true,
            ))
            .unwrap();

        let cache = Arc::new(RwLock::new(PartCache::new()));
        self.rt
            .block_on(app.set_document(cache, &self.document, &|alias, result| {
                match result {
                    Ok(()) => {
                        println!("Loaded part {}.", alias);
                    }
                    Err(e) => {
                        println!("Could not load part {}: {}", alias, e);
                    }
                };
            }));

        app.request_redraw();

        self.app = Some(app);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: event::WindowEvent,
    ) {
        if let Some(app) = &mut self.app {
            match event {
                event::WindowEvent::RedrawRequested => {
                    app.animate(self.started.elapsed().as_millis() as f32 / 1000.0);
                    match app.render() {
                        Ok(duration) => {
                            self.total_duration += duration.as_millis() as i32;
                            self.frames += 1;

                            if self.now.elapsed() > Duration::from_secs(1) {
                                println!(
                                    "{} frames per second. {} msecs per frame.",
                                    self.frames,
                                    self.total_duration as f32 / self.frames as f32
                                );

                                self.now = Instant::now();
                                self.frames = 0;
                                self.total_duration = 0;
                            }
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            app.resize(app.size);
                        }
                        Err(wgpu::SurfaceError::Timeout) => {
                            println!("Surface timeout");
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    }
                }
                event => {
                    app.handle_window_event(
                        event,
                        self.started.elapsed().as_millis() as f32 / 1000.0,
                    );
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(app) = &self.app {
            app.request_redraw()
        }
    }
}

fn main_loop(
    rt: Runtime,
    document: MultipartDocument,
    colors: Rc<ColorCatalog>,
    loader: Rc<HttpLoader>,
) {
    let evloop = EventLoop::new().unwrap();

    let mut app = AppOuter::new(rt, document, loader, colors);

    evloop.run_app(&mut app);
}
