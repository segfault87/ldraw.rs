use std::cmp;
use std::hash;

pub mod color;
pub mod context;
pub mod document;
pub mod elements;
pub mod error;
pub mod library;
#[cfg(target_arch = "wasm32")] pub mod library_wasm;
#[cfg(not(target_arch = "wasm32"))] pub mod library_native;
pub mod parser;
pub mod writer;

#[derive(Debug)]
pub struct NormalizedAlias(String);

impl From<&String> for NormalizedAlias {
    fn from(alias: &String) -> NormalizedAlias {
        NormalizedAlias(alias.trim().to_lowercase().replace("\\", "/"))
    }
}

impl cmp::Eq for NormalizedAlias {}

impl cmp::PartialEq for NormalizedAlias {
    fn eq(&self, other: &NormalizedAlias) -> bool {
        self.0.eq(&other.0)
    }
}

impl hash::Hash for NormalizedAlias {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}
