use futures::future::join_all;
use std::cell::RefCell;
use std::io::BufReader;
use std::rc::Rc;
use std::vec::Vec;

use cgmath::SquareMatrix;
use glow::{Context, HasContext};
use ldraw::color::MaterialRegistry;
use ldraw::document::{Document, MultipartDocument};
use ldraw::library::{PartCache, PartDirectory, ResolutionMap};
use ldraw::parser::{parse_color_definition, parse_multipart_document, parse_single_document};
use ldraw::Matrix4;
use ldraw_renderer::geometry::{BakedModel, ModelBuilder};
use test_renderer::{Program, TestRenderer};
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

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string(), false))
}

macro_rules! console_error {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string(), true))
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
    match JsFuture::from(response.text().unwrap()).await {
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

async fn fetch_multipart_document(
    path: &String,
    colors: &MaterialRegistry,
) -> Result<MultipartDocument, ()> {
    let data = match fetch_raw_data(path).await {
        Ok(v) => v,
        Err(_) => return Err(()),
    };

    match parse_multipart_document(&colors, &mut BufReader::new(data.as_bytes())) {
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

async fn bake(
    document: &MultipartDocument,
    directory: Rc<RefCell<WebPartDirectory>>,
    colors: &MaterialRegistry,
) -> BakedModel {
    let cache = Rc::new(RefCell::new(PartCache::default()));
    let mut resolution = ResolutionMap::new(directory, Rc::clone(&cache));
    resolution.resolve(&&document.body, Some(&document));

    loop {
        let mut futs = Vec::new();
        let mut aliases = Vec::new();
        for (alias, entry) in resolution.get_pending() {
            aliases.push(alias.clone());
            futs.push(fetch_document(&entry.locator, &colors));
        }

        if aliases.len() == 0 {
            break;
        }

        let results = join_all(futs).await;
        for (alias, result) in aliases.iter().zip(results) {
            match result {
                Ok(v) => {
                    console_log!("Loaded subpart {}", &alias.original);
                    cache.borrow_mut().register(alias.clone(), v);
                    resolution.update(&alias, cache.borrow().query(&alias).unwrap());
                }
                Err(_) => {
                    console_error!("Could not load subpart {}", &alias.original);
                }
            };
        }
    }

    console_log!("Loading done.");

    let mut builder = ModelBuilder::new(&colors, &resolution);
    builder.traverse(&&document.body, Matrix4::identity(), true, false);

    builder.bake()
}

async fn build_program<T: HasContext>(
    gl: Rc<RefCell<Box<T>>>,
    vspath: &str,
    fspath: &str,
) -> Result<Program<T>, String> {
    let vsdata = fetch_raw_data(vspath);
    let fsdata = fetch_raw_data(fspath);

    let (vsdata, fsdata) = futures::join!(vsdata, fsdata);
    let vsdata = match vsdata {
        Ok(v) => v,
        Err(_) => {
            return Err(String::from("Could not load vertex program."));
        }
    };
    let fsdata = match fsdata {
        Ok(v) => v,
        Err(_) => {
            return Err(String::from("Could not load fragment program."));
        }
    };

    Program::new(Rc::clone(&gl), &vsdata, &fsdata)
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    let window = web_sys::window().expect("No window exists.");

    window
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
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
    let gl = Rc::new(RefCell::new(Box::new(Context::from_webgl2_context(gl))));

    let default_program = build_program(Rc::clone(&gl), "shaders/default.vs", "shaders/default.fs");
    let edge_program = build_program(Rc::clone(&gl), "shaders/edge.vs", "shaders/edge.fs");
    let (default_program, edge_program) = futures::join!(default_program, edge_program);
    let default_program = match default_program {
        Ok(v) => v,
        Err(_) => {
            console_error!("Couldn't load default shader.");
            return JsValue::undefined();
        }
    };
    let edge_program = match edge_program {
        Ok(v) => v,
        Err(_) => {
            console_error!("Couldn't load edge shader.");
            return JsValue::undefined();
        }
    };
    console_log!("Loaded shader.");

    let directory = fetch_directory();
    let colors = fetch_color_definition();

    let (directory, colors) = futures::join!(directory, colors);
    let directory = match directory {
        Ok(v) => {
            console_log!("Loaded part directory.");
            Rc::new(RefCell::new(v))
        }
        Err(_) => {
            console_error!("Couldn't load part directory.");
            return JsValue::undefined();
        }
    };
    let colors = match colors {
        Ok(v) => {
            console_log!("Loaded color definition.");
            v
        }
        Err(_) => {
            console_error!("Couldn't load color definition.");
            return JsValue::undefined();
        }
    };

    let document = match fetch_multipart_document(&path, &colors).await {
        Ok(v) => {
            console_log!("Loaded model {}.", &path);
            v
        }
        Err(_) => {
            console_error!("Could not load model {}.", &path);
            return JsValue::undefined();
        }
    };
    let model = bake(&document, Rc::clone(&directory), &colors).await;
    console_log!("Reticulated splines.");

    let mut app = TestRenderer::new(&model, Rc::clone(&gl), default_program, edge_program);
    console_log!("Rendering context initialization done.");

    app.resize(canvas.width(), canvas.height());
    console_log!("Ready to go.");

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let mut x = 0.0;
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        x += 1.0 / 60.0;
        app.animate(x);
        app.render();

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

    JsValue::undefined()
}
