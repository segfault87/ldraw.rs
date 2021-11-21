use std::{
    cell::RefCell,
    env,
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
    rc::Rc,
};

use bincode::serialize_into;
use clap::{Arg, App};
use ldraw::{
    color::MaterialRegistry,
    library::{
        load_files,
        scan_ldraw_directory,
        CacheCollectionStrategy,
        PartCache,
        PartDirectoryNative,
        ResolutionMap
    },
    parser::{
        parse_color_definition,
        parse_multipart_document
    },
};
use ldraw_ir::part::{bake_part};

fn main() {
    let matches = App::new("baker")
        .about("Postprocess LDraw model files")
        .arg(Arg::with_name("ldraw_dir")
             .long("ldraw-dir")
             .value_name("PATH")
             .takes_value(true)
             .help("Path to LDraw directory"))
        .arg(Arg::with_name("files")
             .multiple(true)
             .takes_value(true)
             .required(true)
             .help("Files to process"))
        .arg(Arg::with_name("output_path")
             .short("o")
             .long("output-path")
             .takes_value(true)
             .help("Output path"))
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

    let output_path = match matches.value_of("output_path") {
        Some(v) => {
            let path = Path::new(v.clone());
            if !path.is_dir() {
                panic!("{} is not a proper output directory", v);
            }
            Some(path)
        },
        None => None,
    };

    let ldrawpath = Path::new(&ldrawdir);
    let directory = Rc::new(RefCell::new(
        scan_ldraw_directory(&ldrawdir).expect("Not a LDraw path.")
    ));

    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr")).expect("Could not load color definition.")
    )).expect("Could not parse color definition");

    let cache = Rc::new(RefCell::new(PartCache::default()));
    if let Some(files) = matches.values_of("files") {
        for file in files {
            let path = Path::new(&file);
            if !path.exists() {
                panic!("Path {} does not exists.", file);
            } else if path.is_dir() {
                for entry in path.read_dir().expect("Could not read directory.") {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    let ext = path.extension();
                    if ext.is_none() {
                        continue;
                    }
                    let ext = ext.unwrap().to_str().unwrap().to_string().to_lowercase();
                    if ext == "dat" || ext == "ldr" {
                        bake(&colors, Rc::clone(&directory), Rc::clone(&cache), &path, &output_path)
                    }
                }
            } else {
                bake(&colors, Rc::clone(&directory), Rc::clone(&cache), &path, &output_path);
            }
        }
    } else {
        panic!("Required input files are missing.");
    }

    let collected = cache.borrow_mut().collect(CacheCollectionStrategy::PartsAndPrimitives);
    println!("Collected {} entries.", collected);
}

fn bake(colors: &MaterialRegistry,
        directory: Rc<RefCell<PartDirectoryNative>>, cache: Rc<RefCell<PartCache>>, path: &Path,
        output_path: &Option<&Path>) {
    println!("{}", path.to_str().unwrap());

    let document = parse_multipart_document(&colors, &mut BufReader::new(
        File::open(path).expect(&format!("Could not open document {}", path.to_str().unwrap()))
    )).expect(&format!("Could not parse document {}", path.to_str().unwrap()));

    let mut resolution = ResolutionMap::new(Rc::clone(&directory), Rc::clone(&cache));
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

    let part = bake_part(&resolution, None, &document.body);

    let outpath = match output_path {
        Some(e) => {
            e.to_path_buf()
                .join(format!("{}.part", path.file_name().unwrap().to_str().unwrap()))
        },
        None => {
            let mut path_buf = path.to_path_buf();
            path_buf.set_extension(match path.extension() {
                Some(e) => format!("{}.part", e.to_str().unwrap()),
                None => String::from("part"),
            });
            path_buf
        }
    };

    let _ = serialize_into(&mut BufWriter::new(File::create(&outpath).expect(
        &format!("Could not create {}", outpath.to_str().unwrap())
    )), &part);

    drop(resolution);
    drop(document);
    cache.borrow_mut().collect(CacheCollectionStrategy::Parts);
}
