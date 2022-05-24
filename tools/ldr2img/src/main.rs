use std::{
    collections::HashMap,
    env,
    rc::Rc,
    sync::{Arc, RwLock},
};

use async_std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};
use clap::{App, Arg};
use glow::Context as GlContext;
use ldraw::{
    library::{resolve_dependencies_multipart, PartCache},
    parser::{parse_color_definitions, parse_multipart_document},
    resolvers::local::LocalLoader,
    PartAlias,
};
use ldraw_ir::{model::Model, part::bake_part_from_multipart_document};
use ldraw_olr::{
    context::create_offscreen_context,
    ops::render_model,
};
use ldraw_renderer::part::{Part, PartsPool};

#[tokio::main]
async fn main() {
    let matches = App::new("ldr2img")
        .about("Render LDraw model into still image")
        .arg(
            Arg::with_name("ldraw_dir")
                .long("ldraw-dir")
                .value_name("PATH")
                .takes_value(true)
                .help("Path to LDraw directory"),
        )
        .arg(
            Arg::with_name("use_software_renderer")
                .short("w")
                .help("Use software GL context"),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .takes_value(true)
                .help("Output file name"),
        )
        .arg(
            Arg::with_name("input")
                .takes_value(true)
                .required(true)
                .index(1)
                .help("Input file name"),
        )
        .arg(
            Arg::with_name("size")
                .short("s")
                .takes_value(true)
                .help("Maximum width/height pixel size"),
        )
        .get_matches();

    let ldrawdir = match matches.value_of("ldraw_dir") {
        Some(v) => v.to_string(),
        None => match env::var("LDRAWDIR") {
            Ok(v) => v,
            Err(_) => {
                panic!("--ldraw-dir option or LDRAWDIR environment variable is required.");
            }
        },
    };
    let ldraw_path = PathBuf::from(&ldrawdir);

    let use_software_renderer = matches.is_present("use_software_renderer");
    let size = matches
        .value_of("size")
        .unwrap_or("1024")
        .parse::<usize>()
        .unwrap();

    let context = create_offscreen_context(size, size, use_software_renderer).unwrap();

    let gl = Rc::clone(&context.gl);

    let colors = parse_color_definitions(&mut BufReader::new(
        File::open(ldraw_path.join("LDConfig.ldr")).await.unwrap(),
    ))
    .await
    .unwrap();

    let input = matches.value_of("input").unwrap();
    let output = matches.value_of("output").unwrap_or("image.png");

    let document = parse_multipart_document(
        &mut BufReader::new(File::open(&input).await.unwrap()),
        &colors,
    )
    .await
    .unwrap();

    let input_path = PathBuf::from(input);

    let loader = LocalLoader::new(
        Some(ldraw_path),
        Some(PathBuf::from(input_path.parent().unwrap())),
    );

    let cache = Arc::new(RwLock::new(PartCache::new()));
    let resolution_result =
        resolve_dependencies_multipart(&document, Arc::clone(&cache), &colors, &loader, &|_, _| {})
            .await;

    struct PartsPoolImpl(HashMap<PartAlias, Arc<Part<GlContext>>>);
    impl PartsPool<GlContext> for PartsPoolImpl {
        fn query(&self, alias: &PartAlias) -> Option<Arc<Part<GlContext>>> {
            self.0.get(alias).map(Arc::clone)
        }
    }

    let parts = document
        .list_dependencies()
        .into_iter()
        .filter_map(|alias| {
            resolution_result.query(&alias, true).map(|(part, local)| {
                (
                    alias.clone(),
                    Arc::new(Part::create(
                        &bake_part_from_multipart_document(part, &resolution_result, local),
                        Rc::clone(&gl),
                        &colors,
                    )),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    let parts = Arc::new(RwLock::new(PartsPoolImpl(parts)));

    {
        let mut rc = context.rendering_context.borrow_mut();

        rc.set_initial_state();
        rc.resize(size as _, size as _);
        rc.upload_shading_data();
    }

    let model =
        Model::from_ldraw_multipart_document(&document, &colors, Some((&loader, cache))).await;

    let image = render_model(&model, &context, parts, &colors);
    image.save(&Path::new(output)).unwrap();
}
