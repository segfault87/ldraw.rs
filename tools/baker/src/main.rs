use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use bincode::serialize;
use clap::{App, Arg};
use futures::{future::join_all, StreamExt};
use itertools::Itertools;
use ldraw::{
    color::ColorCatalog,
    library::{resolve_dependencies_multipart, CacheCollectionStrategy, LibraryLoader, PartCache},
    parser::{parse_color_definitions, parse_multipart_document},
    resolvers::local::LocalLoader,
};
use ldraw_ir::part::bake_part_from_multipart_document;
use tokio::{
    fs::{self, File},
    io::{AsyncWriteExt, BufReader, BufWriter},
    task::spawn_blocking,
};
use tokio_stream::wrappers::ReadDirStream;

#[tokio::main]
async fn main() {
    let matches = App::new("baker")
        .about("Postprocess LDraw model files")
        .arg(
            Arg::with_name("ldraw_dir")
                .long("ldraw-dir")
                .value_name("PATH")
                .takes_value(true)
                .help("Path to LDraw directory"),
        )
        .arg(
            Arg::with_name("files")
                .multiple(true)
                .takes_value(true)
                .required(true)
                .help("Files to process"),
        )
        .arg(
            Arg::with_name("output_path")
                .short("o")
                .long("output-path")
                .takes_value(true)
                .help("Output path"),
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

    let output_path = match matches.value_of("output_path") {
        Some(v) => {
            let path = Path::new(v);
            if !path.is_dir() {
                panic!("{} is not a proper output directory", v);
            }
            Some(path)
        }
        None => None,
    };

    let ldrawpath = PathBuf::from(&ldrawdir);

    let colors = parse_color_definitions(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr"))
            .await
            .expect("Could not load color definition."),
    ))
    .await
    .expect("Could not parse color definition");

    let loader = LocalLoader::new(Some(ldrawpath), None);

    let mut tasks = vec![];

    let cache = Arc::new(RwLock::new(PartCache::new()));
    if let Some(files) = matches.values_of("files") {
        for file in files {
            let path = PathBuf::from(&file);
            if !fs::try_exists(&path).await.unwrap_or(false) {
                panic!("Path {} does not exists.", file);
            } else if path.is_dir() {
                let mut dir = ReadDirStream::new(
                    fs::read_dir(&path)
                        .await
                        .expect("Could not read directory."),
                );
                while let Some(entry) = dir.next().await {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    let ext = path.extension();
                    if ext.is_none() {
                        continue;
                    }
                    let ext = ext.unwrap().to_str().unwrap().to_string().to_lowercase();
                    if ext == "dat" || ext == "ldr" {
                        tasks.push(bake(
                            &loader,
                            &colors,
                            Arc::clone(&cache),
                            path,
                            &output_path,
                        ));
                    }
                }
            } else {
                tasks.push(bake(
                    &loader,
                    &colors,
                    Arc::clone(&cache),
                    path,
                    &output_path,
                ));
            }
        }
    } else {
        panic!("Required input files are missing.");
    }

    let cpus = num_cpus::get();
    let tasks = tasks
        .into_iter()
        .chunks(cpus)
        .into_iter()
        .map(|chunk| chunk.collect())
        .collect::<Vec<Vec<_>>>();
    for items in tasks {
        join_all(items).await;
    }

    let collected = cache
        .write()
        .unwrap()
        .collect(CacheCollectionStrategy::PartsAndPrimitives);
    println!("Collected {} entries.", collected);
}

async fn bake<L: LibraryLoader>(
    loader: &L,
    colors: &ColorCatalog,
    cache: Arc<RwLock<PartCache>>,
    path: PathBuf,
    output_path: &Option<&Path>,
) {
    println!("{}", path.to_str().unwrap());

    let file = match File::open(path.clone()).await {
        Ok(v) => v,
        Err(err) => {
            println!(
                "Could not open document {}: {}",
                path.to_str().unwrap(),
                err
            );
            return;
        }
    };

    let document = match parse_multipart_document(&mut BufReader::new(file), colors).await {
        Ok(v) => v,
        Err(err) => {
            println!(
                "Could not parse document {}: {}",
                path.to_str().unwrap(),
                err
            );
            return;
        }
    };

    let resolution_result = resolve_dependencies_multipart(
        &document,
        Arc::clone(&cache),
        colors,
        loader,
        &|alias, result| {
            if let Err(err) = result {
                println!("Could not open file {}: {}", alias, err);
            }
        },
    )
    .await;

    let part = spawn_blocking(move || {
        bake_part_from_multipart_document(&document, &resolution_result, false)
    })
    .await
    .unwrap();

    let outpath = match output_path {
        Some(e) => e.to_path_buf().join(format!(
            "{}.part",
            path.file_name().unwrap().to_str().unwrap()
        )),
        None => {
            let mut path_buf = path.to_path_buf();
            path_buf.set_extension(match path.extension() {
                Some(e) => format!("{}.part", e.to_str().unwrap()),
                None => String::from("part"),
            });
            path_buf
        }
    };

    match serialize(&part) {
        Ok(serialized) => match File::create(&outpath).await {
            Ok(file) => {
                let mut writer = BufWriter::new(file);
                writer.write_all(&serialized).await.unwrap();
                writer.shutdown().await.unwrap();
            }
            Err(err) => {
                println!("Could not create {}: {}", outpath.to_str().unwrap(), err);
            }
        },
        Err(err) => {
            println!("Could not bake part {}: {}", path.to_str().unwrap(), err);
        }
    };

    cache
        .write()
        .unwrap()
        .collect(CacheCollectionStrategy::Parts);
}
