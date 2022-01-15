use std::{
    collections::HashMap,
    env,
    rc::Rc,
    sync::{Arc, RwLock}
};

use async_std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};
use clap::{App, Arg};
use glutin::event_loop::EventLoop;
use ldraw::{
    library::{LibraryLoader, PartCache, resolve_dependencies},
    parser::{parse_color_definition, parse_multipart_document},
    resolvers::local::LocalLoader,
};
use ldraw_ir::{
    part::bake_part,
};
use ldraw_olr::{
    context::{create_headless_context, create_osmesa_context},
    ops::render_display_list,
};
use ldraw_renderer::{
    display_list::DisplayList,
    part::Part,
};

#[tokio::main]
async fn main() {
    let matches = App::new("ldr2img")
        .about("Render LDraw model into still image")
        .arg(Arg::with_name("ldraw_dir")
             .long("ldraw-dir")
             .value_name("PATH")
             .takes_value(true)
             .help("Path to LDraw directory"))
        .arg(Arg::with_name("use_window_system")
            .short("w")
            .help("Use window system to utilize GPU rendering"))
        .arg(Arg::with_name("output")
            .short("o")
            .takes_value(true)
            .help("Output file name"))
        .arg(Arg::with_name("input")
            .takes_value(true)
            .required(true)
            .index(1)
            .help("Input file name"))
        .arg(Arg::with_name("size")
            .short("s")
            .takes_value(true)
            .help("Maximum width/height pixel size"))
        .get_matches();

    let ldrawdir = match matches.value_of("ldraw_dir") {
        Some(v) => v.to_string(),
        None => {
            match env::var("LDRAWDIR") {
                Ok(v) => v,
                Err(_) => {
                    panic!("--ldraw-dir option or LDRAWDIR environment variable is required.");
                }
            }
        }
    };
    let ldraw_path = PathBuf::from(&ldrawdir);

    let use_window_system = matches.is_present("use_window_system");
    let size = matches.value_of("size").unwrap_or("1024").parse::<usize>().unwrap();

    let context = if use_window_system {
        let evloop = EventLoop::new();
        create_headless_context(evloop, size, size)
    } else {
        create_osmesa_context(size, size)
    }.unwrap();

    let gl = Rc::clone(&context.gl);

    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldraw_path.join("LDConfig.ldr")).await.unwrap(),
    )).await.unwrap();

    let input = matches.value_of("input").unwrap();
    let output = matches.value_of("output").unwrap_or("image.png");

    let document = parse_multipart_document(
        &colors, &mut BufReader::new(File::open(&input).await.unwrap())
    ).await.unwrap();

    let input_path = PathBuf::from(input);

    let loader: Box<dyn LibraryLoader> = Box::new(LocalLoader::new(Some(ldraw_path), Some(PathBuf::from(input_path.parent().unwrap()))));

    let cache = Arc::new(RwLock::new(PartCache::new()));
    let resolution_result = resolve_dependencies(
        cache,
        &colors,
        &loader,
        &document,
        &|_, _| {}
    ).await;

    let parts = document
        .list_dependencies()
        .into_iter()
        .filter_map(|alias| {
            resolution_result.query(&alias, true).map(|(part, local)| {
                (alias.clone(), Part::create(&bake_part(&resolution_result, None, part, local), Rc::clone(&gl)))
            })
        })
        .collect::<HashMap<_, _>>();

    let mut display_list = DisplayList::from_multipart_document(Rc::clone(&gl), &document);

    {
        let mut rc = context.rendering_context.borrow_mut();

        rc.set_initial_state();
        rc.resize(size as _, size as _);
        rc.upload_shading_data();
    }

    let image = render_display_list(&context, &parts, &mut display_list);
    image.save(&Path::new(output)).unwrap();
}
