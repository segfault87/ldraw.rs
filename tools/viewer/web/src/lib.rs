extern crate console_error_panic_hook;

use std::{
    cell::RefCell,
    panic,
    rc::Rc,
    sync::{Arc, RwLock},
};

use async_std::io::BufReader;
use gloo::events::EventListener;
use glow::Context;
use ldraw::{
    document::MultipartDocument,
    error::ResolutionError,
    library::{CacheCollectionStrategy, LibraryLoader, PartCache},
    parser::parse_multipart_document,
    resolvers::http::HttpLoader,
    PartAlias,
};
use ldraw_renderer::shader::ProgramManager;
use reqwest::{Client, Url};
use uuid::Uuid;
use viewer_common::{App, State};
use wasm_bindgen::{
    prelude::*,
    JsCast
};
use wasm_bindgen_futures::{spawn_local};
use web_sys::{
    HtmlButtonElement, HtmlCanvasElement, HtmlDivElement, HtmlSelectElement, HtmlTextAreaElement,
    WebGl2RenderingContext
};

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

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    let window = web_sys::window().expect("No window exists.");

    window
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}

async fn fetch_raw_data(base_url: &Url, path: &String) -> Option<String> {
    let client = Client::new();

    let url = if path.starts_with("http://") || path.starts_with("https://") {
        Url::parse(&path)
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
        Ok(e) => {
            match e.text().await {
                Ok(e) => Some(e),
                Err(err) => {
                    alert(&format!("Could not fetch from url {}: {}", url, err));
                    None
                }
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
        },
        Err(err) => {
            console_error!("Could not load part {}: {}", alias, err);
        },
    }
}

#[wasm_bindgen]
pub async fn run(path: JsValue) -> JsValue {
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    let window = web_sys::window().expect("No window exists.");
    let web_document = window.document().expect("No document exists.");
    let canvas = web_document
        .get_element_by_id("main_canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    let body = web_document.get_element_by_id("body").unwrap();
    canvas.set_width(body.client_width() as u32);
    canvas.set_height(body.client_height() as u32);
    let gl = match canvas
        .get_context("webgl2")
        .unwrap()
        .unwrap()
        .dyn_into::<WebGl2RenderingContext>()
    {
        Ok(v) => v,
        Err(_) => {
            console_error!("WebGL 2 is not supported on your browser.");
            return JsValue::undefined();
        }
    };
    let gl = Rc::new(Context::from_webgl2_context(gl));

    let program_manager = match ProgramManager::new(Rc::clone(&gl)) {
        Ok(e) => e,
        Err(e) => {
            console_log!("{}", e);
            return JsValue::undefined();
        },
    };

    let document_view = web_document.get_element_by_id("document").unwrap().dyn_into::<HtmlTextAreaElement>().unwrap();

    let mut location = Url::parse(&window.location().href().unwrap()).unwrap();
    location.set_fragment(None);
    
    let loader: Rc<Box<dyn LibraryLoader>> = Rc::new(Box::new(HttpLoader::new(Some(location.join("ldraw/").unwrap()), Some(location.clone()))));

    let materials = match loader.load_materials().await {
        Ok(e) => Rc::new(e),
        Err(err) => {
            console_error!("Could not open material definition: {}", err);
            return JsValue::undefined();
        }
    };

    let app = Rc::new(RefCell::new(App::new(Rc::clone(&gl), Rc::clone(&loader), Rc::clone(&materials), program_manager)));
    console_log!("Rendering context initialization done.");

    let cache = Arc::new(RwLock::new(PartCache::default()));

    app.borrow_mut().resize(canvas.width(), canvas.height());

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

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

            let document = match parse_multipart_document(&*materials, &mut BufReader::new(document_text.as_bytes())).await {
                Ok(v) => v,
                Err(err) => {
                    console_error!("Could not parse document: {}", err);
                    return JsValue::undefined();
                }
            };

            if let Err(err) = app.borrow_mut().set_document(Arc::clone(&cache), &document, &log_part_resolution).await {
                console_error!("Could not load model: {}", err);
            }
            cache.write().unwrap().collect(CacheCollectionStrategy::Parts);

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
                
                app.borrow_mut().set_render_target(
                    if value.is_empty() {
                        None
                    } else {
                        Some(value.parse::<Uuid>().unwrap())
                    }
                );
            }) as Box<dyn FnMut(_)>);
            subparts.add_event_listener_with_callback(
                "change",
                closure.as_ref().unchecked_ref()
            ).unwrap();
            closure.forget();

            document_view.set_value(&document_text);
        }
    }

    let new_doc = Rc::new(RefCell::new(None));
    {
        let document_view = document_view.clone();
        let new_doc = Rc::clone(&new_doc);
        let materials = Rc::clone(&materials);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            let document_view = document_view.clone();
            let new_doc = Rc::clone(&new_doc);
            let materials = Rc::clone(&materials);
            spawn_local(async move {
                let document_text = document_view.value();

                let document = match parse_multipart_document(&*materials, &mut BufReader::new(document_text.as_bytes())).await {
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
        submit_button.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }

    // Mouse events
    {
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            if let Ok(mut a) = app.try_borrow_mut() {
                a.orbit.on_mouse_move(event.offset_x() as f32, event.offset_y() as f32);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
    {
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::MouseEvent| {
            app.borrow_mut().orbit.on_mouse_press(true);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
    {
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::MouseEvent| {
            app.borrow_mut().orbit.on_mouse_press(false);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mouseup", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
    {
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |event: web_sys::WheelEvent| {
            let app = &mut app.borrow_mut();
            app.orbit.radius = (app.orbit.radius + event.delta_y() as f32).clamp(100.0, 10000.0);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("wheel", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }

    // Touch events
    let distance = Rc::new(RefCell::new(0.0f32));
    {
        let distance = Rc::clone(&distance);
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |event: web_sys::TouchEvent| {
            if let Ok(mut a) = app.try_borrow_mut() {
                match event.touches().length() {
                    1 => {
                        let t = event.touches().item(0).unwrap();
                        a.orbit.on_mouse_move(t.page_x() as _, t.page_y() as _);
                    },
                    2 => {
                        let t1 = event.touches().item(0).unwrap();
                        let t2 = event.touches().item(1).unwrap();

                        let x1 = t1.page_x();
                        let y1 = t1.page_y();
                        let x2 = t2.page_x();
                        let y2 = t2.page_y();

                        let sd = (((x2 - x1) as f32).powf(2.0) + ((y2 - y1) as f32).powf(2.0)).sqrt();
                        let pd = *distance.borrow();
                        if pd != 0.0 {
                            let distance_delta = sd - pd;

                            a.orbit.radius = (a.orbit.radius - distance_delta).clamp(100.0, 10000.0);
                        }
                        *distance.borrow_mut() = sd;
                    },
                    _ => {},
                }
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchmove", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
    {
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::TouchEvent| {
            app.borrow_mut().orbit.on_mouse_press(true);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
    {
        let distance = Rc::clone(&distance);
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::TouchEvent| {
            app.borrow_mut().orbit.on_mouse_press(false);
            *distance.borrow_mut() = 0.0;
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchend", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }

    {
        let window = web_sys::window().unwrap();
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |_event: web_sys::UiEvent| {
            let app = &mut app.borrow_mut();
            canvas.set_width(canvas.client_width() as _);
            canvas.set_height(canvas.client_height() as _);
            app.resize(canvas.client_width() as _, canvas.client_height() as _);
        }) as Box<dyn FnMut(_)>);
        window.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }

    {
        let next_button = web_document.get_element_by_id("next-button").unwrap();
        let next_button = JsCast::dyn_ref::<HtmlDivElement>(&next_button).unwrap();
        let a = Rc::clone(&app);
        let closure = EventListener::new(&next_button, "click", move |_event| {
            let window = web_sys::window().unwrap();
            let perf = window.performance().unwrap();

            a.borrow_mut().advance(((perf.now() - start_time) / 1000.0) as f32);
        });
        closure.forget();
    }

    let app = Rc::clone(&app);
    let mut state = State::Finished;
    let new_doc = Rc::clone(&new_doc);
    let cache = Arc::clone(&cache);
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let window = web_sys::window().unwrap();
        let perf = window.performance().unwrap();

        if new_doc.borrow().is_some() {
            let document = MultipartDocument::clone(new_doc.borrow().as_ref().unwrap());
            *new_doc.borrow_mut() = None;
            let app = Rc::clone(&app);
            let cache = Arc::clone(&cache);
            let web_document = web_document.clone();
            spawn_local(async move {
                if let Ok(mut m) = app.try_borrow_mut() {
                    if let Err(err) = m.set_document(Arc::clone(&cache), &document, &log_part_resolution).await {
                        console_error!("Could not reload model: {}", err);
                    };
                    cache.write().unwrap().collect(CacheCollectionStrategy::Parts);

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
                } else {
                    alert("Could not set document");
                }
            });
        }

        if let Ok(mut m) = app.try_borrow_mut() {            
            m.set_up();
            m.animate(((perf.now() - start_time) / 1000.0) as f32);
            m.render();

            if m.state != state {
                let next_button = web_document.get_element_by_id("next-button").unwrap();
                let next_button = JsCast::dyn_ref::<HtmlDivElement>(&next_button).unwrap();
                if m.state == State::Step {
                    next_button.set_class_name("active");
                } else {
                    next_button.set_class_name("");
                }

                state = m.state;
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

    JsValue::undefined()
}
