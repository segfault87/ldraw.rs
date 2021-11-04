use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io::BufReader,
    rc::Rc,
    sync::Arc,
    vec::Vec,
};

use async_trait::async_trait;
use cgmath::SquareMatrix;
use futures::future::join_all;
use glow::Context;
use ldraw::{
    color::MaterialRegistry,
    document::{Document, MultipartDocument},
    library::{
        CacheCollectionStrategy, PartCache, PartDirectory,
        ResolutionMap, ResolutionResult
    },
    parser::{parse_color_definition, parse_multipart_document, parse_single_document},
    Matrix4, PartAlias,
};
use ldraw_ir::{
    MeshGroup,
    part::{PartBuilder, bake_part},
};
use ldraw_renderer::{
    part::Part,
    shader::ProgramManager,
};
use test_renderer::App;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlCanvasElement, Request, RequestInit, Response, WebGl2RenderingContext};

const COLOR_DEFINITION_PATH: &'static str = "LDConfig.ldr";
const PART_DIRECTORY_PATH: &'static str = "directory.json";

type WebPartDirectory = PartDirectory<String>;

fn log(s: &str, error: bool) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let console = document.get_element_by_id("console").unwrap();
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
    let path = path.as_string().unwrap();

    let window = web_sys::window().expect("No window exists.");
    let document = window.document().expect("No document exists.");
    let canvas = document
        .get_element_by_id("main_canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    let body = document.get_element_by_id("body").unwrap();
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

    let app = Rc::new(RefCell::new(App::new(Rc::clone(&gl), program_manager)));
    console_log!("Rendering context initialization done.");

    app.borrow_mut().resize(canvas.width(), canvas.height());

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let window = web_sys::window().unwrap();
    let perf = window.performance().unwrap();
    let start_time = perf.now();

    let document = match fetch_raw_data(&path).await {
        Ok(v) => v,
        Err(_) => {
            console_error!("Could not load url {}", path);
            return JsValue::undefined();
        }
    };

    let (document, features, parts) = match load_document(
        &document, &app.borrow().loaded_parts()
    ).await {
        Ok(v) => v,
        Err(e) => {
            console_error!("{}", e);
            return JsValue::undefined();
        }
    };

    app.borrow_mut().set_document(&document, &features, &parts);

    let app_cloned = Rc::clone(&app);
    let on_mouse_down = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
        let window = web_sys::window().unwrap();
        let perf = window.performance().unwrap();

        app_cloned.borrow_mut().advance(((perf.now() - start_time) / 1000.0) as f32);
    }) as Box<dyn FnMut(_)>);
    canvas.add_event_listener_with_callback("mousedown", on_mouse_down.as_ref().unchecked_ref()).unwrap();
    on_mouse_down.forget();

    let a = Rc::clone(&app);
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let window = web_sys::window().unwrap();
        let perf = window.performance().unwrap();
        
        let mut m = a.borrow_mut();
        m.set_up();
        m.animate(((perf.now() - start_time) / 1000.0) as f32);
        m.render(None);

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

    JsValue::undefined()
}
