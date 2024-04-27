use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use clap::{App, Arg};
use ldraw::{
    library::{resolve_dependencies_multipart, PartCache},
    parser::{parse_color_definitions, parse_multipart_document},
    resolvers::local::LocalLoader,
    PartAlias,
};
use ldraw_ir::{model::Model, part::bake_part_from_multipart_document};
use ldraw_olr::{context::Context, ops::Ops};
use ldraw_renderer::part::{Part, PartQuerier};
use tokio::{fs::File, io::BufReader};

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
                .default_value("1024")
                .takes_value(true)
                .help("Maximum width/height pixel size"),
        )
        .arg(
            Arg::with_name("without-multisample")
                .short("m")
                .help("Number of samples"),
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

    let size = matches.value_of("size").unwrap().parse::<u32>().unwrap();
    let sample_count = if matches.is_present("without-multisample") {
        1
    } else {
        4
    };

    let mut context = Context::new(size, size, sample_count).await.unwrap();

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

    struct PartsPoolImpl(HashMap<PartAlias, Part>);
    impl PartQuerier<PartAlias> for PartsPoolImpl {
        fn get(&self, key: &PartAlias) -> Option<&Part> {
            self.0.get(key)
        }
    }

    let parts = document
        .list_dependencies()
        .into_iter()
        .filter_map(|alias| {
            resolution_result.query(&alias, true).map(|(part, local)| {
                (
                    alias.clone(),
                    Part::new(
                        &bake_part_from_multipart_document(part, &resolution_result, local),
                        &context.device,
                        &colors,
                    ),
                )
            })
        })
        .collect::<HashMap<_, _>>();

    let parts = PartsPoolImpl(parts);

    let model =
        Model::from_ldraw_multipart_document(&document, &colors, Some((&loader, cache))).await;

    let image = {
        let ops = Ops::new(&mut context);
        ops.render_model(&model, None, &parts, &colors).await
    };
    image.save(&Path::new(output)).unwrap();
}
