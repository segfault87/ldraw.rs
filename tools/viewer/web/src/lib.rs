#![cfg(target_arch = "wasm32")]

extern crate console_error_panic_hook;

use std::{
    cell::RefCell,
    panic,
    rc::Rc,
    sync::{Arc, RwLock},
};

use async_std::io::BufReader;
use gloo::events::EventListener;
use ldraw::{
    document::MultipartDocument,
    error::ResolutionError,
    library::{CacheCollectionStrategy, LibraryLoader, PartCache},
    parser::parse_multipart_document,
    resolvers::http::HttpLoader,
    PartAlias,
};
use reqwest::{Client, Url};
use uuid::Uuid;
use viewer_common::{App, State};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    HtmlButtonElement, HtmlCanvasElement, HtmlDivElement, HtmlSelectElement, HtmlTextAreaElement,
};
use winit::{
    event,
    event_loop::{ControlFlow, EventLoop},
    platform::web::WindowBuilderExtWebSys,
    window::WindowBuilder,
};

// A huge mess. Needs refactoring.

const ANTIALIAS: bool = if cfg!(feature = "webgl") { false } else { true };

fn log(s: &str, error: bool) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let console = document.get_element_by_id("console-pane").unwrap();
    let node = document.create_element("p").unwrap();
    node.set_attribute(
        "class",
        match error {
            false => "log",
            true => "error",
        },
    )
    .unwrap();
    node.set_inner_html(s);
    console.prepend_with_node_1(&node).unwrap();
}

fn alert(s: &str) {
    let window = web_sys::window().unwrap();
    window.alert_with_message(s).unwrap();
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string(), false))
}

macro_rules! console_error {
    ($($t:tt)*) => (alert(&format_args!($($t)*).to_string()))
}

async fn fetch_raw_data(base_url: &Url, path: &String) -> Option<String> {
    let client = Client::new();

    let url = if path.starts_with("http://") || path.starts_with("https://") {
        Url::parse(path)
    } else {
        base_url.join(path)
    };

    let url = match url {
        Ok(e) => e,
        Err(err) => {
            alert(&format!("Could not build url {}: {}", path, err));
            return None;
        }
    };

    match client.get(url.clone()).send().await {
        Ok(e) => match e.text().await {
            Ok(e) => Some(e),
            Err(err) => {
                alert(&format!("Could not fetch from url {}: {}", url, err));
                None
            }
        },
        Err(err) => {
            alert(&format!("Could not make request to {}: {}", url, err));
            None
        }
    }
}

fn log_part_resolution(alias: PartAlias, result: Result<(), ResolutionError>) {
    match result {
        Ok(_) => {
            console_log!("Part {} loaded", alias);
        }
        Err(err) => {
            console_error!("Could not load part {}: {}", alias, err);
        }
    }
}

#[wasm_bindgen]
#[allow(clippy::await_holding_refcell_ref)]
pub async fn run(path: JsValue) -> JsValue {
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    let web_window = web_sys::window().expect("No window exists.");
    let web_document = web_window.document().expect("No document exists.");
    let body = web_document.get_element_by_id("body").unwrap();
    let canvas = web_document
        .get_element_by_id("main_canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(body.client_width() as u32);
    canvas.set_height(body.client_height() as u32);

    let event_loop = EventLoop::new();
    let builder = WindowBuilder::new()
        .with_title("ldraw.rs demo")
        .with_inner_size(winit::dpi::LogicalSize {
            width: body.client_width() as u32,
            height: body.client_height() as u32,
        })
        .with_canvas(Some(canvas));

    let window = builder.build(&event_loop).unwrap();

    let canvas = web_document
        .get_element_by_id("main_canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();

    let document_view = web_document
        .get_element_by_id("document")
        .unwrap()
        .dyn_into::<HtmlTextAreaElement>()
        .unwrap();

    let mut location = Url::parse(&web_window.location().href().unwrap()).unwrap();
    location.set_fragment(None);

    let loader: Rc<dyn LibraryLoader> = Rc::new(HttpLoader::new(
        Some(location.join("ldraw/").unwrap()),
        Some(location.clone()),
    ));

    let colors = match loader.load_colors().await {
        Ok(e) => Rc::new(e),
        Err(err) => {
            console_error!("Could not open color definitions: {}", err);
            return JsValue::undefined();
        }
    };

    let app = match App::new(window, Rc::clone(&loader), Rc::clone(&colors), ANTIALIAS).await {
        Ok(v) => v,
        Err(e) => {
            console_error!("Could not initialize the app: {e}");
            return JsValue::undefined();
        }
    };

    let app = Rc::new(RefCell::new(app));
    console_log!("Rendering context initialization done.");

    let cache = Arc::new(RwLock::new(PartCache::default()));

    app.borrow_mut().resize(winit::dpi::PhysicalSize {
        width: canvas.width(),
        height: canvas.height(),
    });

    let window = web_sys::window().unwrap();
    let perf = window.performance().unwrap();
    let start_time = perf.now();

    {
        let document_view = document_view.clone();
        if path.is_string() {
            let path = path.as_string().unwrap();
            let document_text = match fetch_raw_data(&location, &path).await {
                Some(v) => v,
                None => {
                    return JsValue::undefined();
                }
            };

            let document = match parse_multipart_document(
                &mut BufReader::new(document_text.as_bytes()),
                &*colors,
            )
            .await
            {
                Ok(v) => v,
                Err(err) => {
                    console_error!("Could not parse document: {}", err);
                    return JsValue::undefined();
                }
            };

            if let Err(err) = app
                .borrow_mut()
                .set_document(Arc::clone(&cache), &document, &log_part_resolution)
                .await
            {
                console_error!("Could not load model: {}", err);
            }
            cache
                .write()
                .unwrap()
                .collect(CacheCollectionStrategy::Parts);

            let subparts = web_document.get_element_by_id("subparts").unwrap();
            subparts.set_inner_html("");

            let body = web_document.create_element("option").unwrap();
            body.set_attribute("value", "").unwrap();
            body.set_inner_html("Base Model");
            subparts.append_child(&body).unwrap();

            for (id, name) in app.borrow().get_subparts() {
                let subpart = web_document.create_element("option").unwrap();
                subpart.set_attribute("value", &format!("{}", id)).unwrap();
                subpart.set_inner_html(&format!("Subpart {} ({})", name, id));
                subparts.append_child(&subpart).unwrap();
            }

            let web_document = web_document.clone();
            let app = Rc::clone(&app);
            let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                let subparts = web_document.get_element_by_id("subparts").unwrap();
                let subparts = JsCast::dyn_ref::<HtmlSelectElement>(&subparts).unwrap();
                let value = subparts.value();

                app.borrow_mut().set_render_target(if value.is_empty() {
                    None
                } else {
                    Some(value.parse::<Uuid>().unwrap())
                });
            }) as Box<dyn FnMut(_)>);
            subparts
                .add_event_listener_with_callback("change", closure.as_ref().unchecked_ref())
                .unwrap();
            closure.forget();

            document_view.set_value(&document_text);
        }
    }

    let new_doc = Rc::new(RefCell::new(None));
    {
        let document_view = document_view.clone();
        let new_doc = Rc::clone(&new_doc);
        let colors = Rc::clone(&colors);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            let document_view = document_view.clone();
            let new_doc = Rc::clone(&new_doc);
            let colors = Rc::clone(&colors);
            spawn_local(async move {
                let document_text = document_view.value();

                let document = match parse_multipart_document(
                    &mut BufReader::new(document_text.as_bytes()),
                    &*colors,
                )
                .await
                {
                    Ok(v) => v,
                    Err(err) => {
                        console_error!("Could not parse document: {}", err);
                        return;
                    }
                };

                *new_doc.borrow_mut() = Some(document);
            });
        }) as Box<dyn FnMut(_)>);
        let submit_button = web_document.get_element_by_id("submit").unwrap();
        let submit_button = JsCast::dyn_ref::<HtmlButtonElement>(&submit_button).unwrap();
        submit_button
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }

    {
        let window = web_sys::window().unwrap();
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::UiEvent| {
            let app = &mut app.borrow_mut();
            canvas.set_width(canvas.client_width() as _);
            canvas.set_height(canvas.client_height() as _);
            app.resize(winit::dpi::PhysicalSize {
                width: canvas.client_width() as _,
                height: canvas.client_height() as _,
            });
        }) as Box<dyn FnMut(_)>);
        window
            .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }

    {
        let next_button = web_document.get_element_by_id("next-button").unwrap();
        let next_button = JsCast::dyn_ref::<HtmlDivElement>(&next_button).unwrap();
        let a = Rc::clone(&app);
        let closure = EventListener::new(next_button, "click", move |_event| {
            let window = web_sys::window().unwrap();
            let perf = window.performance().unwrap();

            a.borrow_mut()
                .advance(((perf.now() - start_time) / 1000.0) as f32);
        });
        closure.forget();
    }

    {
        let app = Rc::clone(&app);
        let new_doc = Rc::clone(&new_doc);
        let web_document = web_document.clone();
        let cache = Arc::clone(&cache);

        event_loop.run(move |event, _, control_flow| match event {
            event::Event::LoopDestroyed => {}
            event::Event::RedrawRequested(_) => {
                let mut app_ = app.borrow_mut();

                app_.animate(((perf.now() - start_time) / 1000.0) as f32);
                match app_.render() {
                    Ok(duration) => {
                        let next_button = web_document.get_element_by_id("next-button").unwrap();
                        let next_button = JsCast::dyn_ref::<HtmlDivElement>(&next_button).unwrap();
                        if app_.state() == State::Step {
                            next_button.set_class_name("active");
                        } else {
                            next_button.set_class_name("");
                        }

                        let backend = if cfg!(feature = "webgl") {
                            "WebGL"
                        } else {
                            "WebGPU"
                        };

                        let stats = web_document.get_element_by_id("stats").unwrap();
                        let stats = JsCast::dyn_ref::<HtmlDivElement>(&stats).unwrap();
                        stats.set_inner_html(&format!(
                            "Rendering backend: {}<br />{} msecs",
                            backend,
                            duration.as_millis(),
                        ));
                    }
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        //app.resize(app.size);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        *control_flow = ControlFlow::Exit;
                    }
                    Err(wgpu::SurfaceError::Timeout) => {
                        println!("Surface timeout");
                    }
                }

                if new_doc.borrow().is_some() {
                    let document = MultipartDocument::clone(new_doc.borrow().as_ref().unwrap());
                    *new_doc.borrow_mut() = None;

                    let app = Rc::clone(&app);
                    let cache = Arc::clone(&cache);
                    let web_document = web_document.clone();

                    spawn_local(async move {
                        if let Err(err) = app
                            .borrow_mut()
                            .set_document(Arc::clone(&cache), &document, &log_part_resolution)
                            .await
                        {
                            console_error!("Could not reload model: {}", err);
                        };

                        cache
                            .write()
                            .unwrap()
                            .collect(CacheCollectionStrategy::Parts);

                        let subparts = web_document.get_element_by_id("subparts").unwrap();
                        subparts.set_inner_html("");

                        let body = web_document.create_element("option").unwrap();
                        body.set_attribute("value", "").unwrap();
                        body.set_inner_html("Base Model");
                        subparts.append_child(&body).unwrap();

                        console_log!("{:?}", body);

                        for (id, name) in app.borrow().get_subparts() {
                            let subpart = web_document.create_element("option").unwrap();
                            subpart.set_attribute("value", &format!("{}", id)).unwrap();
                            subpart.set_inner_html(&format!("Subpart {} ({})", name, id));
                            subparts.append_child(&subpart).unwrap();
                        }
                    });
                }
            }
            event::Event::NewEvents(
                event::StartCause::Init | event::StartCause::ResumeTimeReached { .. },
            ) => {
                if let Ok(app) = app.try_borrow() {
                    app.request_redraw();
                }
            }
            event::Event::MainEventsCleared => {
                if let Ok(app) = app.try_borrow() {
                    app.request_redraw();
                }
            }
            event::Event::WindowEvent { event, .. } => {
                if let event::WindowEvent::Resized(s) = event {
                    console_log!("Resized {:?}", s);
                }
                if let Ok(mut app) = app.try_borrow_mut() {
                    app.handle_window_event(event, ((perf.now() - start_time) / 1000.0) as f32);
                }
            }
            _ => (),
        });
    }
}
