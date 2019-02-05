use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use ldraw::library::{load_files, scan_ldraw_directory, PartCache, ResolutionMap};
use ldraw::parser::{parse_color_definition, parse_multipart_document};

fn main() {
    let ldrawdir = match env::var("LDRAWDIR") {
        Ok(val) => val,
        Err(e) => panic!("{}", e),
    };
    let ldrawpath = Path::new(&ldrawdir);

    let directory = scan_ldraw_directory(&ldrawdir).unwrap();
    let colors = parse_color_definition(&mut BufReader::new(
        File::open(ldrawpath.join("LDConfig.ldr")).unwrap(),
    ))
    .unwrap();

    let ldrpath = match env::args().skip(1).next() {
        Some(e) => e,
        None => panic!("usage: loader [filename]"),
    };

    let document =
        parse_multipart_document(&colors, &mut BufReader::new(File::open(ldrpath).unwrap()))
            .unwrap();

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

    for d in document.body.iter_refs() {
        println!("{:#?}", resolution.query(d));
    }
}
