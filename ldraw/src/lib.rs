#![feature(trait_alias)]

use std::cmp;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::ops::BitXor;

use cgmath::{Matrix3 as Matrix3_, Matrix4 as Matrix4_, Vector3 as Vector3_, Vector4 as Vector4_};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{Error as DeserializeError, Visitor};

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

pub type Matrix3 = Matrix3_<f32>;
pub type Matrix4 = Matrix4_<f32>;
pub type Vector3 = Vector3_<f32>;
pub type Vector4 = Vector4_<f32>;

pub trait AliasType = Clone + Debug;

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

impl From<&str> for NormalizedAlias {
    fn from(alias: &str) -> NormalizedAlias {
        let string = alias.to_string();
        
        NormalizedAlias {
            normalized: Self::normalize(&string),
            original: string,
        }
    }
}

struct StringVisitor;

impl<'a> Visitor<'a> for StringVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut Formatter) -> FmtResult {
        write!(formatter, "a string")
    }

    fn visit_str<E: DeserializeError>(self, v: &str) -> Result<Self::Value, E> {
        Ok(String::from(v))
    }
}

impl Serialize for NormalizedAlias {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.original.as_str())
    }
}

impl<'a> Deserialize<'a> for NormalizedAlias {
    fn deserialize<D: Deserializer<'a>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(NormalizedAlias::from(&deserializer.deserialize_str(StringVisitor)?))
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

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Winding {
    Ccw,
    Cw,
}

impl Winding {
    pub fn invert(&self) -> Self {
        match self {
            Winding::Ccw => Winding::Cw,
            Winding::Cw => Winding::Ccw,
        }
    }
}

impl BitXor<bool> for Winding {
    type Output = Self;

    fn bitxor(self, rhs: bool) -> Self::Output {
        match (self, rhs) {
            (Winding::Ccw, false) => Winding::Ccw,
            (Winding::Ccw, true) => Winding::Cw,
            (Winding::Cw, false) => Winding::Cw,
            (Winding::Cw, true) => Winding::Ccw,
        }
    }
}

impl BitXor<bool> for &Winding {
    type Output = Winding;

    fn bitxor(self, rhs: bool) -> Self::Output {
        match (self, rhs) {
            (Winding::Ccw, false) => Winding::Ccw,
            (Winding::Ccw, true) => Winding::Cw,
            (Winding::Cw, false) => Winding::Cw,
            (Winding::Cw, true) => Winding::Ccw,
        }
    }
}
