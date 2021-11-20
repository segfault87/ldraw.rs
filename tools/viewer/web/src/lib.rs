extern crate console_error_panic_hook;

use std::{
    cell::{RefCell},
    collections::{HashMap, HashSet},
    io::BufReader,
    panic,
    rc::Rc,
    vec::Vec,
};

use futures::future::join_all;
use gloo::events::EventListener;
use glow::Context;
use ldraw::{
    color::MaterialRegistry,
    document::{Document, MultipartDocument},
    library::{
        CacheCollectionStrategy, PartCache, PartDirectory,
        ResolutionMap, ResolutionResult
    },
    parser::{parse_color_definition, parse_multipart_document, parse_single_document},
    PartAlias,
};
use ldraw_ir::part::{PartBuilder, bake_part};
use ldraw_renderer::shader::ProgramManager;
use viewer_common::{App, State};
use wasm_bindgen::{
    prelude::*,
    JsCast
};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    HtmlButtonElement, HtmlCanvasElement, HtmlDivElement, HtmlInputElement, HtmlTextAreaElement,
    Request, RequestInit, Response,
    WebGl2RenderingContext
};

const COLOR_DEFINITION_PATH: &'static str = "LDConfig.ldr";
const PART_DIRECTORY_PATH: &'static str = "directory.json";

type WebPartDirectory = PartDirectory<String>;

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
    window.alert_with_message(s);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string(), false))
}

macro_rules! console_error {
    ($($t:tt)*) => (alert(&format_args!($($t)*).to_string()))
}

async fn load_document(
    resource: &String, loaded: &HashSet<&PartAlias>
) -> Result<(MultipartDocument, HashMap<PartAlias, PartBuilder>, HashMap<PartAlias, PartBuilder>), &'static str> {
    let enabled_features = get_features_list();
    let directory = fetch_directory();
    let colors = fetch_color_definition();

    let (directory, colors) = futures::join!(directory, colors);
    let directory = match directory {
        Ok(v) => {
            console_log!("Loaded part directory.");
            v
        }
        Err(_) => {
            return Err("Couldn't load part directory.");
        }
    };
    let colors = match colors {
        Ok(v) => {
            console_log!("Loaded color definition.");
            v
        }
        Err(_) => {
            return Err("Couldn't load color definition.");
        }
    };

    let document = match parse_multipart_document(&colors, &mut BufReader::new(resource.as_bytes())) {
        Ok(v) => {
            console_log!("Loaded model.");
            v
        }
        Err(_) => {
            return Err("Could not parse document. (maybe corrupt?)");
        }
    };

    let cache = Rc::new(RefCell::new(PartCache::default()));
    let mut resolution = ResolutionMap::new(Rc::new(RefCell::new(directory)), Rc::clone(&cache));
    resolution.resolve(&&document.body, Some(&document));

    loop {
        let mut futs = Vec::new();
        let mut aliases = Vec::new();
        for (alias, entry) in resolution.get_pending() {
            aliases.push((alias.clone(), entry.kind.clone()));
            futs.push(fetch_document(&entry.locator, &colors));
        }

        if aliases.len() == 0 {
            break;
        }

        let results = join_all(futs).await;
        for (alias, result) in aliases.iter().zip(results) {
            match result {
                Ok(v) => {
                    console_log!("Loaded subpart {}", &alias.0.original);
                    cache.borrow_mut().register(alias.1.clone(), alias.0.clone(), v);
                    resolution.update(&alias.0, cache.borrow().query(&alias.0).unwrap());
                }
                Err(_) => {
                    console_error!("Could not load subpart {}", &alias.0.original);
                }
            };
        }
    }

    console_log!("Loading done.");

    let mut features = HashMap::new();
    let mut deps = HashMap::new();

    for feature in enabled_features.iter() {
        if loaded.contains(&feature) {
            continue;
        }
        let part = resolution.map.get(&feature);
        if part.is_none() {
            println!("Dependency {} has not been found", feature);
            continue;
        }
        let part = part.unwrap();
        let element = match part {
            ResolutionResult::Associated(e) => {
                e
            }
            _ => {
                console_log!("Could not bake dependency {}", feature);
                continue;
            }
        };
        console_log!("Processed feature {}.", feature);
        features.insert(feature.clone(), bake_part(&resolution, None, &element));
    }
    for dep in document.list_dependencies() {
        if loaded.contains(&dep) {
            continue;
        }
        let part = resolution.map.get(&dep);
        if part.is_none() {
            println!("Dependency {} has not been found", dep);
            continue;
        }
        let part = part.unwrap();
        let element = match part {
            ResolutionResult::Associated(e) => {
                e
            }
            _ => {
                println!("Could not bake dependency {}", dep);
                continue;
            }
        };
        console_log!("Processed dependent part {}.", dep);
        deps.insert(dep.clone(), bake_part(&resolution, Some(&enabled_features), &element));
    }

    drop(resolution);

    console_log!(
        "Collected {} entries",
        cache
            .borrow_mut()
            .collect(CacheCollectionStrategy::PartsAndPrimitives)
    );

    console_log!("All parts built.");

    Ok((document, features, deps))
}

async fn do_request(url: &str) -> Result<Response, ()> {
    let window = web_sys::window().unwrap();
    let mut opts = RequestInit::new();
    opts.method("GET");

    let request = match Request::new_with_str_and_init(url, &opts) {
        Ok(v) => v,
        Err(_) => return Err(()),
    };
    let response_value = match JsFuture::from(window.fetch_with_request(&request)).await {
        Ok(v) => v,
        Err(_) => return Err(()),
    };

    match response_value.dyn_into() {
        Ok(v) => Ok(v),
        Err(_) => Err(()),
    }
}

async fn fetch_raw_data(url: &str) -> Result<String, ()> {
    let response = match do_request(url).await {
        Ok(v) => v,
        Err(_) => return Err(()),
    };
    let text = response.text().unwrap();
    match JsFuture::from(text).await {
        Ok(v) => Ok(v.as_string().unwrap()),
        Err(_) => Err(()),
    }
}

async fn fetch_directory() -> Result<WebPartDirectory, ()> {
    let response = match do_request(PART_DIRECTORY_PATH).await {
        Ok(v) => v,
        Err(_) => return Err(()),
    };
    let json = JsFuture::from(response.json().unwrap()).await.unwrap();
    let directory: WebPartDirectory = json.into_serde().unwrap();

    Ok(directory)
}

async fn fetch_color_definition() -> Result<MaterialRegistry, ()> {
    let data = match fetch_raw_data(COLOR_DEFINITION_PATH).await {
        Ok(v) => v,
        Err(_) => return Err(()),
    };

    match parse_color_definition(&mut BufReader::new(data.as_bytes())) {
        Ok(v) => Ok(v),
        Err(_) => Err(()),
    }
}

async fn fetch_document(path: &String, colors: &MaterialRegistry) -> Result<Document, ()> {
    let data = match fetch_raw_data(path).await {
        Ok(v) => v,
        Err(_) => return Err(()),
    };

    match parse_single_document(&colors, &mut BufReader::new(data.as_bytes())) {
        Ok(v) => Ok(v),
        Err(_) => Err(()),
    }
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    let window = web_sys::window().expect("No window exists.");

    window
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}

fn get_features_list() -> HashSet<PartAlias> {
    let mut features = HashSet::new();
    //features.insert(PartAlias::from(String::from("stud.dat")));

    features
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

    let app = Rc::new(RefCell::new(App::new(Rc::clone(&gl), program_manager)));
    console_log!("Rendering context initialization done.");

    app.borrow_mut().resize(canvas.width(), canvas.height());

    let slider = web_document.get_element_by_id("slider").unwrap();
    let slider = JsCast::dyn_ref::<HtmlInputElement>(&slider).unwrap();

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let window = web_sys::window().unwrap();
    let perf = window.performance().unwrap();
    let start_time = perf.now();

    {
        let document_view = document_view.clone();
        if path.is_string() {
            let path = path.as_string().unwrap();
            let document_text = match fetch_raw_data(&path).await {
                Ok(v) => v,
                Err(_) => {
                    console_error!("Could not load url {}", path);
                    return JsValue::undefined();
                }
            };

            let (document, features, parts) = match load_document(
                &document_text, &app.borrow().loaded_parts()
            ).await {
                Ok(v) => v,
                Err(e) => {
                    console_error!("{}", e);
                    return JsValue::undefined();
                }
            };

            app.borrow_mut().set_document(&document, &features, &parts);

            document_view.set_value(&document_text);

            let part_count = &app.borrow().part_count();
            slider.set_max(&part_count.to_string());
            slider.set_value(&part_count.to_string());
        }
    }

    let new_doc = Rc::new(RefCell::new(None));
    {
        let app = Rc::clone(&app);
        let document_view = document_view.clone();
        let slider = slider.clone();
        let new_doc = Rc::clone(&new_doc);
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let document_view = document_view.clone();
            let app = Rc::clone(&app);
            let slider = slider.clone();
            let new_doc = Rc::clone(&new_doc);
            spawn_local(async move {
                let (document, features, parts) = match load_document(
                    &document_view.value(), &app.borrow().loaded_parts()
                ).await {
                    Ok(v) => v,
                    Err(e) => {
                        console_error!("{}", e);
                        return;
                    }
                };

                *new_doc.borrow_mut() = Some((document, features, parts));
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
        let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            app.borrow_mut().orbit.on_mouse_press(true);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
    {
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
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
    let tx = Rc::new(RefCell::new(0));
    let ty = Rc::new(RefCell::new(0));
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
        let closure = Closure::wrap(Box::new(move |event: web_sys::TouchEvent| {
            app.borrow_mut().orbit.on_mouse_press(true);
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }
    {
        let tx = Rc::clone(&tx);
        let ty = Rc::clone(&ty);
        let distance = Rc::clone(&distance);
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |event: web_sys::TouchEvent| {
            app.borrow_mut().orbit.on_mouse_press(false);
            *distance.borrow_mut() = 0.0;
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchend", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }

    {
        let window = web_sys::window().unwrap();
        let app = Rc::clone(&app);
        let closure = Closure::wrap(Box::new(move |event: web_sys::UiEvent| {
            let app = &mut app.borrow_mut();
            let window = web_sys::window().unwrap();
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
        let closure = EventListener::new(&next_button, "click", move |event| {
            let window = web_sys::window().unwrap();
            let perf = window.performance().unwrap();

            a.borrow_mut().advance(((perf.now() - start_time) / 1000.0) as f32);
        });
        closure.forget();
    }
    {
        let app = Rc::clone(&app);
        let slider_ = slider.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let value = slider_.value().parse::<usize>().unwrap_or(0);
            if let Ok(mut app) = app.try_borrow_mut() {
                app.rebuild_display_list(value);
            }
        }) as Box<dyn FnMut(_)>);
        slider.add_event_listener_with_callback("input", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
    }

    let app = Rc::clone(&app);
    let mut state = State::Finished;
    let slider_ = slider.clone();
    let new_doc = Rc::clone(&new_doc);
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let window = web_sys::window().unwrap();
        let perf = window.performance().unwrap();

        if let Ok(mut m) = app.try_borrow_mut() {
            if let Some((document, features, parts)) = &*new_doc.borrow() {
                m.set_document(&document, &features, &parts);

                let part_count = m.part_count();
                slider_.set_max(&part_count.to_string());
                slider_.set_value(&part_count.to_string());
                slider_.style().set_property("display", "none");
            }
            *new_doc.borrow_mut() = None;
            
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

                if m.state == State::Finished {
                    slider_.style().set_property("display", "block");
                }

                state = m.state;
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

    JsValue::undefined()
}
