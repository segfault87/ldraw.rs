use std::cmp;
use std::hash::{Hash, Hasher};

use cgmath::{Matrix4 as Matrix4_, Vector3 as Vector3_, Vector4 as Vector4_};

pub mod color;
pub mod document;
pub mod elements;
pub mod error;
pub mod library;
#[cfg(not(target_arch = "wasm32"))]
pub mod library_native;
#[cfg(target_arch = "wasm32")]
pub mod library_wasm;
pub mod parser;
pub mod writer;

pub type Matrix4 = Matrix4_<f32>;
pub type Vector3 = Vector3_<f32>;
pub type Vector4 = Vector4_<f32>;

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

    pub fn normalize(alias: &str) -> String {
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

impl Hash for NormalizedAlias {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.normalized.hash(state)
    }
}
