#![feature(trait_alias)]

use std::cmp;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::hash::{Hash, Hasher};
use std::ops::BitXor;

use cgmath::{
    Matrix3 as Matrix3_, Matrix4 as Matrix4_, Point2 as Point2_, Point3 as Point3_,
    Vector2 as Vector2_, Vector3 as Vector3_, Vector4 as Vector4_,
};
use serde::de::{Error as DeserializeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod color;
pub mod document;
pub mod elements;
pub mod error;
pub mod library;
#[cfg(not(target_arch = "wasm32"))]
pub mod library_native;
pub mod parser;
pub mod writer;

pub type Matrix3 = Matrix3_<f32>;
pub type Matrix4 = Matrix4_<f32>;
pub type Vector2 = Vector2_<f32>;
pub type Vector3 = Vector3_<f32>;
pub type Vector4 = Vector4_<f32>;
pub type Point2 = Point2_<f32>;
pub type Point3 = Point3_<f32>;

pub trait AliasType = Clone + Debug;

#[derive(Clone, Debug)]
pub struct PartAlias {
    pub normalized: String,
    pub original: String,
}

impl PartAlias {
    pub fn set(&mut self, alias: String) {
        self.normalized = Self::normalize(&alias);
        self.original = alias;
    }

    pub fn normalize(alias: &str) -> String {
        alias.trim().to_lowercase().replace("\\", "/")
    }
}

impl From<String> for PartAlias {
    fn from(alias: String) -> PartAlias {
        PartAlias {
            normalized: Self::normalize(&alias),
            original: alias,
        }
    }
}

impl From<&String> for PartAlias {
    fn from(alias: &String) -> PartAlias {
        PartAlias {
            normalized: Self::normalize(alias),
            original: alias.clone(),
        }
    }
}

impl From<&str> for PartAlias {
    fn from(alias: &str) -> PartAlias {
        let string = alias.to_string();

        PartAlias {
            normalized: Self::normalize(&string),
            original: string,
        }
    }
}

impl Display for PartAlias {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        Display::fmt(&self.original, f)
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

impl Serialize for PartAlias {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.original.as_str())
    }
}

impl<'a> Deserialize<'a> for PartAlias {
    fn deserialize<D: Deserializer<'a>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(PartAlias::from(
            &deserializer.deserialize_str(StringVisitor)?,
        ))
    }
}

impl cmp::Eq for PartAlias {}

impl cmp::PartialEq for PartAlias {
    fn eq(&self, other: &PartAlias) -> bool {
        self.normalized.eq(&other.normalized)
    }
}

impl Hash for PartAlias {
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
    pub fn invert(self) -> Self {
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
