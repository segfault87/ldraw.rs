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

#[derive(Clone, Debug)]
pub struct NormalizedAlias {
    normalized: String,
    pub original: String,
}

impl NormalizedAlias {
    pub fn set(&mut self, alias: String) {
        self.normalized = Self::normalize(&alias);
        self.original = alias;
    }

    pub fn normalize(alias: &String) -> String {
        alias.trim().to_lowercase().replace("\\", "/")
    }
}

impl From<String> for NormalizedAlias {
    fn from(alias: String) -> NormalizedAlias {
        NormalizedAlias {
            normalized: Self::normalize(&alias),
            original: alias,
        }
    }
}

impl From<&String> for NormalizedAlias {
    fn from(alias: &String) -> NormalizedAlias {
        NormalizedAlias {
            normalized: Self::normalize(alias),
            original: alias.clone(),
        }
    }
}

impl cmp::Eq for NormalizedAlias {}

impl cmp::PartialEq for NormalizedAlias {
    fn eq(&self, other: &NormalizedAlias) -> bool {
        self.normalized.eq(&other.normalized)
    }
}

impl hash::Hash for NormalizedAlias {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.normalized.hash(state)
    }
}
