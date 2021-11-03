use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::BufReader,
    path::Path,
    rc::Rc,
    str,
    time::Instant,
};

use cgmath::SquareMatrix;
use glow::{self, Context, HasContext};
use glutin::{
    dpi::LogicalSize,
    ContextBuilder, ElementState, Event, EventsLoop, GlProfile, GlRequest,
    KeyboardInput, VirtualKeyCode, WindowBuilder, WindowEvent
};
use ldraw::{
    color::MaterialRegistry,
    document::MultipartDocument,
    library::{
        load_files, scan_ldraw_directory, CacheCollectionStrategy, PartCache,
        PartDirectoryNative, ResolutionMap, ResolutionResult,
    },
    parser::{parse_color_definition, parse_multipart_document},
    Matrix4, PartAlias
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

fn bake(
    colors: &MaterialRegistry,
    directory: Rc<RefCell<PartDirectoryNative>>,
    path: &str,
    enabled_features: &HashSet<PartAlias>,
) -> (MultipartDocument, HashMap<PartAlias, PartBuilder>, HashMap<PartAlias, PartBuilder>) {
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

    let mut features = HashMap::new();
    let mut deps = HashMap::new();
    for feature in enabled_features.iter() {
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
                println!("Could not bake dependency {}", feature);
                continue;
            }
        };
        features.insert(feature.clone(), bake_part(&resolution, None, &element));
    }
    for dep in document.list_dependencies() {
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
        deps.insert(dep.clone(), bake_part(&resolution, Some(&enabled_features), &element));
    }

    drop(resolution);

    println!(
        "Collected {} entries",
        cache
            .borrow_mut()
            .collect(CacheCollectionStrategy::PartsAndPrimitives)
    );

    (document, features, deps)
}

fn main_loop(
    document: MultipartDocument,
    colors: &MaterialRegistry,
    features: HashMap<PartAlias, PartBuilder>,
    parts: HashMap<PartAlias, PartBuilder>
) {
    let mut evloop = EventsLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("ldraw.rs demo")
        .with_dimensions(LogicalSize::new(1280.0, 720.0));
    let windowed_context = ContextBuilder::new()
        .with_gl_profile(GlProfile::Core)
        .with_gl(GlRequest::Latest)
        .with_multisampling(4)
        .with_vsync(true)
        .build_windowed(window_builder, &evloop)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };
    let gl = unsafe { Context::from_loader_function(|s| windowed_context.get_proc_address(s) as *const _) };
    let gl = Rc::new(gl);

    let program_manager = match ProgramManager::new(Rc::clone(&gl)) {
        Ok(e) => e,
        Err(e) => panic!("{}", e),
    };

    let features = features.iter().map(|(k, v)| (k.clone(), Part::create(&v, Rc::clone(&gl)))).collect::<HashMap<_, _>>();
    let parts = parts.iter().map(|(k, v)| (k.clone(), Part::create(&v, Rc::clone(&gl)))).collect::<HashMap<_, _>>();

    let mut app = App::new(Rc::clone(&gl), document, features, parts, program_manager);

    let window = windowed_context.window();
    let size = window.get_inner_size().unwrap();
    let (w, h) = size.to_physical(window.get_hidpi_factor()).into();
    app.resize(w, h);

    let mut closed = false;
    let started = Instant::now();
    app.set_up();
    while !closed {
        app.animate(started.elapsed().as_millis() as f32 / 1000.0);
        app.render();

        windowed_context.swap_buffers().unwrap();

        evloop.poll_events(|event| {
            match event {
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::CloseRequested => {
                            closed = true;
                        }
                        WindowEvent::Resized(size) => {
                            let physical = size.to_physical(window.get_hidpi_factor());
                            windowed_context.resize(physical);
                            let (w, h): (u32, u32) = physical.into();
                            app.resize(w, h);
                        }
                        WindowEvent::KeyboardInput { input, .. } => {
                            if input.virtual_keycode == Some(VirtualKeyCode::Space) && input.state == ElementState::Pressed {
                                app.advance(started.elapsed().as_millis() as f32 / 1000.0);
                            }
                        }
                        _ => (),
                    }
                },
                _ => (),
            }
        });
    }
}

fn get_part_size(part: &PartBuilder) -> usize {
    let mut bytes = 0;

    bytes += part.part_builder.uncolored_mesh.len() * 3 * 4 * 2;
    bytes += part.part_builder.uncolored_without_bfc_mesh.len() * 3 * 4 * 2;
    for (group, mesh) in part.part_builder.opaque_meshes.iter() {
        bytes += mesh.len() * 3 * 4 * 2;
    }
    for (group, mesh) in part.part_builder.semitransparent_meshes.iter() {
        bytes += mesh.len() * 3 * 4 * 2;
    }
    bytes += part.part_builder.edges.len() * 3 * 4 * 2;
    bytes += part.part_builder.optional_edges.len() * 3 * 4 * 2;

    bytes
}

fn get_features_list() -> HashSet<PartAlias> {
    let mut features = HashSet::new();
    //features.insert(PartAlias::from(String::from("stud.dat")));

    features
}

fn main() {
    let enabled_features = get_features_list();

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

    let (doc, features, deps) = bake(&colors, directory, &ldrpath, &enabled_features);

    let mut total_bytes: usize = 0;
    for (_, part) in features.iter() {
        total_bytes += get_part_size(&part);
    }
    for (_, part) in deps.iter() {
        total_bytes += get_part_size(&part);
    }

    println!("Total bytes: {:.2} MB", total_bytes as f32 / 1048576.0);

    main_loop(doc, &colors, features, deps);
}
