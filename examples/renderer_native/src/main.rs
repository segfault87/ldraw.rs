use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::rc::Rc;
use std::str;
use std::time::Instant;

use cgmath::SquareMatrix;
use glow::{self, Context, HasContext};
use glutin::dpi::LogicalSize;
use glutin::{ContextBuilder, Event, EventsLoop, WindowBuilder, WindowEvent};
use ldraw::color::MaterialRegistry;
use ldraw::library::{
    load_files, scan_ldraw_directory, PartCache, PartDirectoryNative, ResolutionMap,
};
use ldraw::parser::{parse_color_definition, parse_multipart_document};
use ldraw::Matrix4;
use ldraw_renderer::geometry::{BakedModel, ModelBuilder};
use test_renderer::{Program, TestRenderer};

fn bake(
    colors: &MaterialRegistry,
    directory: Rc<RefCell<PartDirectoryNative>>,
    path: &str,
) -> BakedModel {
    println!("Parsing document...");
    let document =
        parse_multipart_document(&colors, &mut BufReader::new(File::open(path).unwrap())).unwrap();

    println!("Resolving dependencies...");
    let cache = Rc::new(RefCell::new(PartCache::default()));
    let mut resolution = ResolutionMap::new(directory, Rc::clone(&cache));
    resolution.resolve(&&document.body, Some(&document));
    loop {
        let files = match load_files(&colors, Rc::clone(&cache), resolution.get_pending()) {
            Some(e) => e,
            None => break,
        };
        for key in files {
            let doc = cache.borrow().query(&key).unwrap();
            resolution.update(&key, doc);
        }
    }

    println!("Baking model...");

    let mut builder = ModelBuilder::new(&resolution);
    builder.traverse(&&document.body, Matrix4::identity(), true, false);
    let model = builder.bake();

    drop(builder);
    drop(resolution);
    drop(document);

    println!("Collected {} entries", cache.borrow_mut().collect());

    model
}

fn set_up_context(gl: &Context) {
    unsafe {
        gl.clear_color(1.0, 1.0, 1.0, 1.0);
        gl.line_width(1.0);
        gl.cull_face(glow::BACK);
        gl.enable(glow::CULL_FACE);
        gl.enable(glow::DEPTH_TEST);
        gl.enable(glow::BLEND);
        gl.depth_func(glow::LEQUAL);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
    }
}

fn main_loop(model: &BakedModel) {
    let mut evloop = EventsLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("ldraw.rs demo")
        .with_dimensions(LogicalSize::new(1280.0, 720.0));
    let windowed_context = ContextBuilder::new()
        .with_vsync(true)
        .build_windowed(window_builder, &evloop)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };
    let gl = Context::from_loader_function(|s| windowed_context.get_proc_address(s) as *const _);
    set_up_context(&gl);

    let gl = Rc::new(RefCell::new(Box::new(gl)));

    let edge_program = Program::new(
        Rc::clone(&gl),
        &String::from(str::from_utf8(include_bytes!("../../renderer/shaders/edge.vs")).unwrap()),
        &String::from(str::from_utf8(include_bytes!("../../renderer/shaders/edge.fs")).unwrap()),
    )
    .unwrap();
    let default_program = Program::new(
        Rc::clone(&gl),
        &String::from(str::from_utf8(include_bytes!("../../renderer/shaders/default.vs")).unwrap()),
        &String::from(str::from_utf8(include_bytes!("../../renderer/shaders/default.fs")).unwrap()),
    )
    .unwrap();

    let mut app = TestRenderer::new(model, Rc::clone(&gl), default_program, edge_program);
    let window = windowed_context.window();
    let size = window.get_inner_size().unwrap();
    let (w, h) = size.to_physical(window.get_hidpi_factor()).into();
    app.resize(w, h);

    let mut closed = false;
    let started = Instant::now();
    while !closed {
        set_up_context(&*gl.borrow());
        
        app.animate(started.elapsed().as_millis() as f32 / 1000.0);
        app.render();

        unsafe {
            (*gl.borrow()).flush();
        }

        windowed_context.swap_buffers().unwrap();

        evloop.poll_events(|event| {
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested => {
                        closed = true;
                    }
                    WindowEvent::Resized(size) => {
                        let physical = size.to_physical(window.get_hidpi_factor());
                        windowed_context.resize(physical);
                        let (w, h) = physical.into();
                        app.resize(w, h);
                    }
                    _ => (),
                }
            }
        });
    }
}

fn main() {
    let ldrawdir = match env::var("LDRAWDIR") {
        Ok(val) => val,
        Err(e) => panic!("{}", e),
    };
    let ldrawpath = Path::new(&ldrawdir);

    println!("Scanning LDraw directory...");
    let directory = Rc::new(RefCell::new(scan_ldraw_directory(&ldrawdir).unwrap()));

    println!("Loading color definition...");
    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr")).unwrap(),
    ))
    .unwrap();

    let ldrpath = match env::args().nth(1) {
        Some(e) => e,
        None => panic!("usage: loader [filename]"),
    };

    let baked = bake(&colors, directory, &ldrpath);

    main_loop(&baked);
}
