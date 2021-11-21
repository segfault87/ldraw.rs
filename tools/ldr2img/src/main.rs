use std::{
    collections::HashMap,
    env,
    fs::File,
    io::BufReader,
    path::Path,
    rc::Rc,
};

use bincode::deserialize_from;
use clap::{App, Arg};
use glutin::event_loop::EventLoop;
use ldraw::{
    parser::{parse_color_definition, parse_multipart_document},
};
use ldraw_ir::{
    part::PartBuilder,
};
use ldraw_olr::{
    context::{create_headless_context, create_osmesa_context},
    ops::render_display_list,
};
use ldraw_renderer::{
    display_list::DisplayList,
    part::Part,
};

fn main() {
    let matches = App::new("ldr2img")
        .about("Render LDraw model into still image")
        .arg(Arg::with_name("ldraw_dir")
             .long("ldraw-dir")
             .value_name("PATH")
             .takes_value(true)
             .help("Path to LDraw directory"))
        .arg(Arg::with_name("parts_path")
            .short("p")
            .value_name("PATH")
            .takes_value(true)
            .help("Path to baked LDraw parts"))
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
    let ldrawdir = Path::new(&ldrawdir);

    let bakeddir = match matches.value_of("parts_path") {
        Some(v) => Path::new(v).to_path_buf(),
        None => {
            let baked = Path::new(&ldrawdir).join("baked");
            if baked.exists() {
                baked
            } else {
                panic!("Parts path is not provided.")
            }
        }
    };

    let use_window_system = matches.is_present("use_window_system");
    let size = matches.value_of("size").unwrap_or("1024").parse::<usize>().unwrap();

    let mut context = if use_window_system {
        let evloop = EventLoop::new();
        create_headless_context(evloop, size, size)
    } else {
        create_osmesa_context(size, size)
    }.unwrap();

    let gl = Rc::clone(&context.gl);

    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawdir.join("LDConfig.ldr")).unwrap(),
    )).unwrap();

    let input = matches.value_of("input").unwrap();
    let output = matches.value_of("output").unwrap_or("image.png");

    let document = parse_multipart_document(
        &colors, &mut BufReader::new(File::open(&input).unwrap())
    ).unwrap();

    let mut parts = HashMap::new();
    for dep in document.list_dependencies() {
        let path = bakeddir.join(format!("{}.part", dep.normalized));
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => {
                println!("Could not open part file {}.", path.to_str().unwrap_or(""));
                continue
            },
        };
        let mut part = deserialize_from::<_, PartBuilder>(&mut BufReader::new(file)).unwrap();
        part.part_builder.resolve_colors(&colors);
        let part = Part::create(&part, Rc::clone(&gl));
        parts.insert(dep.clone(), part);
    }

    let mut display_list = DisplayList::from_multipart_document(Rc::clone(&gl), &document);

    let rc = &mut context.rendering_context;

    rc.set_initial_state();
    rc.resize(size as _, size as _);
    rc.upload_shading_data();

    let image = render_display_list(&mut context, &parts, &mut display_list);
    image.save(&Path::new(output)).unwrap();
}
