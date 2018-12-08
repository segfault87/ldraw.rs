use std::collections::HashMap;

use crate::NormalizedAlias;
use crate::document::{Document, MultipartDocument};

#[derive(Clone, Copy, Debug)]
pub enum PartKind {
    Primitive,
    Part,
}

#[derive(Debug)]
pub struct PartEntry<T> {
    pub kind: PartKind,
    pub locator: T,
}

pub struct PartDirectory<T> {
    pub primitives: HashMap<NormalizedAlias, PartEntry<T>>,
    pub parts: HashMap<NormalizedAlias, PartEntry<T>>,
}

impl<T> PartDirectory<T> {
    pub fn new() -> PartDirectory<T> {
        PartDirectory {
            primitives: HashMap::new(),
            parts: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: NormalizedAlias, entry: PartEntry<T>) {
        match entry.kind {
            PartKind::Primitive => self.primitives.insert(key, entry),
            PartKind::Part => self.parts.insert(key, entry),
        };
    }

    pub fn query(&self, key: &NormalizedAlias) -> Option<&PartEntry<T>> {
        match self.parts.get(key) {
            Some(v) => Some(v),
            None => {
                match self.primitives.get(key) {
                    Some(v) => Some(v),
                    None => None
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use crate::library_wasm::*;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::library_native::*;
