use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env,
    fs::File,
    io::BufReader,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

use glow::{self, Context};
use glutin::{
    dpi::{LogicalSize, Size},
    event::{ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder, GlProfile, GlRequest,
};
use ldraw::{
    color::MaterialRegistry,
    document::MultipartDocument,
    library::{
        load_files, scan_ldraw_directory, CacheCollectionStrategy, PartCache,
        ResolutionMap, ResolutionResult,
    },
    parser::{parse_color_definition, parse_multipart_document},
    PartAlias
};
use ldraw_ir::{
    part::{PartBuilder, bake_part},
};
use ldraw_renderer::{
    shader::ProgramManager,
};
use viewer_common::App;

struct NativeLoader {
    ldrawdir: String,
    colors: MaterialRegistry,
    enabled_features: HashSet<PartAlias>,
}

fn get_part_size(part: &PartBuilder) -> usize {
    let mut bytes = 0;

    bytes += part.part_builder.uncolored_mesh.len() * 3 * 4 * 2;
    bytes += part.part_builder.uncolored_without_bfc_mesh.len() * 3 * 4 * 2;
    for (_, mesh) in part.part_builder.opaque_meshes.iter() {
        bytes += mesh.len() * 3 * 4 * 2;
    }
    for (_, mesh) in part.part_builder.translucent_meshes.iter() {
        bytes += mesh.len() * 3 * 4 * 2;
    }
    bytes += part.part_builder.edges.len() * 3 * 4 * 2;
    bytes += part.part_builder.optional_edges.len() * 3 * 4 * 2;

    bytes
}

impl NativeLoader {

    fn load(
        &mut self, locator: &String, loaded: &HashSet<&PartAlias>
    ) -> (MultipartDocument, HashMap<PartAlias, PartBuilder>, HashMap<PartAlias, PartBuilder>) {
        println!("Scanning LDraw directory...");
        let directory = Rc::new(RefCell::new(scan_ldraw_directory(&self.ldrawdir).unwrap()));

        println!("Parsing document...");
        let document =
            parse_multipart_document(&self.colors, &mut BufReader::new(File::open(&locator).unwrap())).unwrap();

        println!("Resolving dependencies...");
        let cache = Rc::new(RefCell::new(PartCache::default()));
        let mut resolution = ResolutionMap::new(directory, Rc::clone(&cache));
        resolution.resolve(&&document.body, Some(&document));
        loop {
            let files = match load_files(&self.colors, Rc::clone(&cache), resolution.get_pending()) {
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
        for feature in self.enabled_features.iter() {
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
                    println!("Could not bake dependency {}", feature);
                    continue;
                }
            };
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
            deps.insert(dep.clone(), bake_part(&resolution, Some(&self.enabled_features), &element));
        }

        drop(resolution);

        println!(
            "Collected {} entries",
            cache
                .borrow_mut()
                .collect(CacheCollectionStrategy::PartsAndPrimitives)
        );

        let mut total_bytes: usize = 0;
        for (_, part) in features.iter() {
            total_bytes += get_part_size(&part);
        }
        for (_, part) in deps.iter() {
            total_bytes += get_part_size(&part);
        }

        println!("Total bytes: {:.2} MB", total_bytes as f32 / 1048576.0);

        (document, features, deps)
    }

}

fn main_loop(
    locator: &String,
    mut resource_loader: NativeLoader
) {
    let evloop = EventLoop::new();
    let window_builder = WindowBuilder::new()
        .with_title("ldraw.rs demo");
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

    let mut app = App::new(Rc::clone(&gl), program_manager);

    let (document, features, parts) = resource_loader.load(&locator, &HashSet::new());
    app.set_document(&document, &features, &parts);

    let window = windowed_context.window();
    let size = window.inner_size();
    app.resize(size.width, size.height);

    let started = Instant::now();
    app.set_up();

    evloop.run(move |event, _, control_flow| {
        match event {
            Event::LoopDestroyed => return,
            Event::RedrawRequested(_) => {
                app.render();

                windowed_context.swap_buffers().unwrap();
            },
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                    }
                    WindowEvent::Resized(size) => {
                        println!("size {:?}", size);
                        app.resize(size.width, size.height);
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        if input.virtual_keycode == Some(VirtualKeyCode::Space) && input.state == ElementState::Pressed {
                            app.advance(started.elapsed().as_millis() as f32 / 1000.0);
                        }
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if button == MouseButton::Left {
                            println!("press {:?}", state);
                            app.orbit.on_mouse_press(state == ElementState::Pressed);
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        println!("moved {:?}", position);
                        app.orbit.on_mouse_move(position.x as f32, position.y as f32)
                    }
                    _ => ()
                }
            },
            _ => (),
        }

        app.animate(started.elapsed().as_millis() as f32 / 1000.0);
        app.render();
        windowed_context.swap_buffers().unwrap();

        let next_frame_time = Instant::now() + Duration::from_nanos(16_666_667);
        *control_flow = ControlFlow::WaitUntil(next_frame_time);
    });
        
}

fn get_features_list() -> HashSet<PartAlias> {
    //let mut features = HashSet::new();
    //features.insert(PartAlias::from(String::from("stud.dat")));

    //features

    HashSet::new()
}

fn main() {
    let ldrawdir = match env::var("LDRAWDIR") {
        Ok(val) => val,
        Err(e) => panic!("{}", e),
    };
    let ldrawpath = Path::new(&ldrawdir);

    println!("Loading color definition...");
    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr")).unwrap(),
    ))
    .unwrap();

    let ldrpath = match env::args().nth(1) {
        Some(e) => e,
        None => panic!("usage: loader [filename]"),
    };

    let enabled_features = get_features_list();
    let resource_loader = NativeLoader {
        ldrawdir, colors, enabled_features
    };

    main_loop(&ldrpath, resource_loader);
}
