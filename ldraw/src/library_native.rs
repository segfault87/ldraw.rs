use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::NormalizedAlias;
use crate::error::LibraryError;
use crate::library::{PartDirectory, PartEntry, PartKind};

pub type PartEntryNative = PartEntry<OsString>;
pub type PartDirectoryNative = PartDirectory<OsString>;

impl From<&OsString> for NormalizedAlias {
    fn from(e: &OsString) -> NormalizedAlias {
        NormalizedAlias::from(&e.to_string_lossy().to_owned().to_string())
    }
}

fn scan_directory(
    basepath: &PathBuf, relpath: PathBuf, mut dir: &mut HashMap<NormalizedAlias, PartEntryNative>,
    kind: &'static PartKind
) -> Result<(), LibraryError> {
    for entry_ in basepath.read_dir()? {
        let entry = entry_?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            scan_directory(&entry.path(), relpath.join(path.file_name().unwrap()), &mut dir, kind)?;
        } else {
            let key = relpath.join(path.file_name().unwrap());
            let alias = NormalizedAlias::from(&key.into_os_string());
            dir.insert(alias, PartEntryNative {
                kind: kind.clone(),
                locator: path.into_os_string(),
            });
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

    let mut dir = PartDirectoryNative::new();
    scan_directory(&path_parts, PathBuf::new(), &mut dir.parts, &PartKind::Part)?;
    scan_directory(&path_primitives, PathBuf::new(), &mut dir.primitives, &PartKind::Primitive)?;

    Ok(dir)
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
            },
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
    }
    
}
