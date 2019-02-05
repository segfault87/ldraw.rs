use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ldraw::color::MaterialRegistry;
use ldraw::library::{
    load_files, scan_ldraw_directory, PartCache, PartDirectoryNative, ResolutionMap,
};
use ldraw::parser::{parse_color_definition, parse_multipart_document};
use ldraw_renderer::geometry::{bake_model, BakedModel};

fn bake(colors: &MaterialRegistry, directory: &PartDirectoryNative, path: &str) -> BakedModel {
    println!("Parsing document...");
    let document =
        parse_multipart_document(&colors, &mut BufReader::new(File::open(path).unwrap())).unwrap();

    println!("Resolving dependencies...");
    let cache = RefCell::new(PartCache::default());
    let mut resolution = ResolutionMap::new(&directory, &cache);
    resolution.resolve(&&document.body, Some(&document));
    loop {
        let files = match load_files(&colors, &cache, resolution.get_pending()) {
            Some(e) => e,
            None => break,
        };
        for key in files {
            let doc = cache.borrow().query(&key).unwrap();
            resolution.update(&key, doc);
        }
    }

    println!("Baking model...");
    let model = bake_model(&colors, &resolution, &mut HashMap::new(), &document.body);

    drop(resolution);
    drop(document);

    println!("Collected {} entries", cache.borrow_mut().collect());

    model
}

fn main() {
    let ldrawdir = match env::var("LDRAWDIR") {
        Ok(val) => val,
        Err(e) => panic!("{}", e),
    };
    let ldrawpath = Path::new(&ldrawdir);

    println!("Scanning LDraw directory...");
    let directory = scan_ldraw_directory(&ldrawdir).unwrap();

    println!("Loading color definition...");
    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr")).unwrap(),
    ))
    .unwrap();

    let ldrpath = match env::args().nth(1) {
        Some(e) => e,
        None => panic!("usage: loader [filename]"),
    };

    let baked = bake(&colors, &directory, &ldrpath);

    println!("Result:");
    for (group, mesh) in baked.meshes.iter() {
        println!("    {:?}: {} vertices", group, mesh.vertices.len() / 3);
    }
    println!("    {} edges", baked.edges.vertices.len() / 3);

    println!("Rendering order:");
    let mut keys = baked.meshes.keys().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        println!("    {:?}", key);
    }
}
