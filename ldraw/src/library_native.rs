use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use crate::{
    color::MaterialRegistry,
    error::LibraryError,
    library::{PartCache, PartDirectory, PartEntry, PartKind},
    parser::parse_single_document,
    PartAlias,
};

pub type PartEntryNative = PartEntry<OsString>;
pub type PartDirectoryNative = PartDirectory<OsString>;

impl From<&OsString> for PartAlias {
    fn from(e: &OsString) -> PartAlias {
        PartAlias::from(&e.to_string_lossy().to_owned().to_string())
    }
}

fn scan_directory(
    basepath: &Path,
    relpath: PathBuf,
    dir: &mut HashMap<PartAlias, PartEntryNative>,
    kind: PartKind,
) -> Result<(), LibraryError> {
    for entry in basepath.read_dir()? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if !path.is_dir() && path.extension().and_then(OsStr::to_str) != Some("dat") {
            continue;
        }
        if file_type.is_dir() {
            scan_directory(
                &entry.path(),
                relpath.join(path.file_name().unwrap()),
                dir,
                kind,
            )?;
        } else {
            let key = relpath.join(path.file_name().unwrap());
            let alias = PartAlias::from(&key.into_os_string());
            dir.insert(
                alias,
                PartEntryNative {
                    kind,
                    locator: path.into_os_string(),
                },
            );
        }
    }

    Ok(())
}

pub fn scan_ldraw_directory(path_str: &str) -> Result<PartDirectoryNative, LibraryError> {
    let path = Path::new(path_str);

    let path_parts = path.join("parts");
    let path_primitives = path.join("p");

    if !path_parts.exists() || !path_primitives.exists() {
        return Err(LibraryError::NoLDrawDir);
    }

    let mut dir = PartDirectoryNative::default();
    scan_directory(&path_parts, PathBuf::new(), &mut dir.parts, PartKind::Part)?;
    scan_directory(
        &path_primitives,
        PathBuf::new(),
        &mut dir.primitives,
        PartKind::Primitive,
    )?;

    Ok(dir)
}

pub fn load_files<'a, T>(
    materials: &MaterialRegistry,
    cache: Arc<RwLock<PartCache>>,
    files: T,
) -> Option<Vec<PartAlias>>
where
    T: Iterator<Item = (&'a PartAlias, &'a PartEntryNative)>,
{
    let mut loaded = Vec::new();

    for (alias, entry) in files {
        let file = match File::open(&entry.locator) {
            Ok(v) => v,
            Err(e) => {
                println!("Could not open part file {}: {:?}", alias.original, e);
                continue;
            }
        };
        let result = match parse_single_document(materials, &mut BufReader::new(file)) {
            Ok(v) => v,
            Err(e) => {
                println!("Could not read part file {}: {:?}", alias.original, e);
                continue;
            }
        };

        cache.write().unwrap().register(entry.kind, alias.clone(), result);
        loaded.push(alias.clone());
    }

    if loaded.is_empty() {
        None
    } else {
        Some(loaded)
    }
}

#[cfg(test)]
mod tests {
    const LDRAW_DIR: &'static str = "/home/segfault/.ldraw";

    use super::scan_ldraw_directory;

    #[test]
    fn test_scan_ldraw_directory() {
        match scan_ldraw_directory(LDRAW_DIR) {
            Ok(v) => {
                println!("{:#?}", v.primitives);
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
    }
}
