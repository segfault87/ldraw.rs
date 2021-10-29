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
    vec::Vec,
};
use cgmath::{Deg, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix};
use glow::{self, Context, HasContext};
use glutin::{
    dpi::LogicalSize,
    ContextBuilder, Event, EventsLoop, GlProfile, GlRequest,
    WindowBuilder, WindowEvent
};
use ldraw::{
    color::{
        ColorReference, Material, MaterialRegistry
    },
    library::{
        load_files, scan_ldraw_directory,
        CacheCollectionStrategy, PartCache, PartDirectoryNative,
        ResolutionMap, ResolutionResult
    },
    parser::{parse_color_definition, parse_multipart_document},
    Vector3, Vector4, Matrix3, Matrix4, PartAlias
};
use ldraw_ir::{
    MeshGroup,
    part::{PartBuilder, bake_part},
};
use ldraw_renderer::{
    error::RendererError,
    state::{RenderingContext},
    shader::{Bindable, ProgramManager},
};

fn bake(
    colors: &MaterialRegistry,
    directory: Rc<RefCell<PartDirectoryNative>>,
    path: &str,
    enabled_features: &HashSet<PartAlias>,
) -> (HashMap<PartAlias, PartBuilder>, HashMap<PartAlias, PartBuilder>) {
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
    drop(document);

    println!(
        "Collected {} entries",
        cache
            .borrow_mut()
            .collect(CacheCollectionStrategy::PartsAndPrimitives)
    );

    (features, deps)
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

fn main_loop(colors: &MaterialRegistry) {
    let mut evloop = EventsLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("ldraw.rs demo")
        .with_dimensions(LogicalSize::new(1280.0, 720.0));
    let windowed_context = ContextBuilder::new()
        .with_gl_profile(GlProfile::Core)
        .with_gl(GlRequest::Latest)
        .with_vsync(true)
        .build_windowed(window_builder, &evloop)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };
    let gl = unsafe { Context::from_loader_function(|s| windowed_context.get_proc_address(s) as *const _) };
    set_up_context(&gl);

    let gl = Rc::new(gl);

    let program_manager = ProgramManager::new(Rc::clone(&gl), 1, 0);
    match program_manager {
        Ok(_) => println!("Yay!"),
        Err(e) => println!("{}", e),
    }

    /*let window = windowed_context.window();
    let size = window.get_inner_size().unwrap();
    let (w, h) = size.to_physical(window.get_hidpi_factor()).into();
    app.resize(w, h);

    println!("Bounding box: {:?}", model.bounding_box);

    let mut closed = false;
    let started = Instant::now();
    while !closed {
        set_up_context(&*gl);

        app.animate(started.elapsed().as_millis() as f32 / 1000.0);
        app.render();

        unsafe {
            (*gl).flush();
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
    }*/
}

fn get_features_list() -> HashSet<PartAlias> {
    let mut features = HashSet::new();
    features.insert(PartAlias::from(String::from("stud.dat")));

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

    let (features, deps) = bake(&colors, directory, &ldrpath, &enabled_features);

    let mut total_bytes: usize = 0;
    for (key, part) in features.iter() {
        println!("Feature {}", key);
        if part.part_builder.uncolored_mesh.len() > 0 {
            total_bytes += part.part_builder.uncolored_mesh.len() * 3 * 4 * 2;
            println!("  Uncolored: {}", part.part_builder.uncolored_mesh.len());
        }
        for (group, mesh) in part.part_builder.opaque_meshes.iter() {
            total_bytes += mesh.len() * 3 * 4 * 2;
            println!("  Opaque color {} / bfc {}: {}", group.color_ref.code(), group.bfc, mesh.len());
        }
        for (group, mesh) in part.part_builder.semitransparent_meshes.iter() {
            total_bytes += mesh.len() * 3 * 4 * 2;
            println!("  Semitransparent color {} / bfc {}: {}", group.color_ref.code(), group.bfc, mesh.len());
        }
        total_bytes += part.part_builder.edges.len() * 3 * 4 * 2;
        println!("  Edges: {}", part.part_builder.edges.len());
        total_bytes += part.part_builder.optional_edges.len() * 3 * 4 * 2;
        println!("  Optional edges: {}", part.part_builder.optional_edges.len());
    }
    for (key, part) in deps.iter() {
        println!("Part {}", key);
        if part.part_builder.uncolored_mesh.len() > 0 {
            total_bytes += part.part_builder.uncolored_mesh.len() * 3 * 4 * 2;
            println!("  Uncolored: {}", part.part_builder.uncolored_mesh.len());
        }
        for (group, mesh) in part.part_builder.opaque_meshes.iter() {
            total_bytes += mesh.len() * 3 * 4 * 2;
            println!("  Opaque color {} / bfc {}: {}", group.color_ref.code(), group.bfc, mesh.len());
        }
        for (group, mesh) in part.part_builder.semitransparent_meshes.iter() {
            total_bytes += mesh.len() * 3 * 4 * 2;
            println!("  Semitransparent color {} / bfc {}: {}", group.color_ref.code(), group.bfc, mesh.len());
        }
        total_bytes += part.part_builder.edges.len() * 3 * 4 * 2;
        println!("  Edges: {}", part.part_builder.edges.len());
        total_bytes += part.part_builder.optional_edges.len() * 3 * 4 * 2;
        println!("  Optional edges: {}", part.part_builder.optional_edges.len());
    }

    println!("Total bytes: {:.2} MB", total_bytes as f32 / 1048576.0);

    main_loop(&colors);
}
