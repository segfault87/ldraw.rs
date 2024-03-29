use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
};

use ldraw::color::{ColorCatalog, ColorReference};
use serde::{
    de::{Deserializer, Error as DeError, Unexpected, Visitor},
    ser::Serializer,
    Deserialize, Serialize,
};

pub mod constraints;
pub mod geometry;
pub mod model;
pub mod part;

#[derive(Clone, Debug)]
pub struct MeshGroupKey {
    pub color_ref: ColorReference,
    pub bfc: bool,
}

impl Serialize for MeshGroupKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.bfc {
            serializer.serialize_str(&self.color_ref.code().to_string())
        } else {
            serializer.serialize_str(&format!("!{}", self.color_ref.code()))
        }
    }
}

impl<'de> Deserialize<'de> for MeshGroupKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MeshGroupVisitor;

        impl<'de> Visitor<'de> for MeshGroupVisitor {
            type Value = MeshGroupKey;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a string with number in it and optional exclamation mark preceding to it",
                )
            }

            fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
                let (slice, bfc) = if let Some(stripped) = value.strip_prefix('!') {
                    (stripped, false)
                } else {
                    (value, true)
                };

                match slice.parse::<u32>() {
                    Ok(v) => Ok(MeshGroupKey {
                        color_ref: ColorReference::Unknown(v),
                        bfc,
                    }),
                    Err(_) => Err(DeError::invalid_value(
                        Unexpected::Str(value),
                        &"a string with number in it and optional exclamation mark preceding to it",
                    )),
                }
            }
        }

        deserializer.deserialize_str(MeshGroupVisitor)
    }
}

impl MeshGroupKey {
    pub fn resolve_color(&mut self, colors: &ColorCatalog) {
        self.color_ref.resolve_self(colors);
    }
}

impl Hash for MeshGroupKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color_ref.code().hash(state);
        self.bfc.hash(state);
    }
}

impl PartialOrd for MeshGroupKey {
    fn partial_cmp(&self, other: &MeshGroupKey) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MeshGroupKey {
    fn cmp(&self, other: &MeshGroupKey) -> Ordering {
        let lhs_translucent = match &self.color_ref {
            ColorReference::Color(c) => c.is_translucent(),
            _ => false,
        };
        let rhs_translucent = match &other.color_ref {
            ColorReference::Color(c) => c.is_translucent(),
            _ => false,
        };

        match (lhs_translucent, rhs_translucent) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            (_, _) => (),
        };

        match self.color_ref.code().cmp(&other.color_ref.code()) {
            Ordering::Equal => self.bfc.cmp(&other.bfc),
            e => e,
        }
    }
}

impl Eq for MeshGroupKey {}

impl PartialEq for MeshGroupKey {
    fn eq(&self, other: &MeshGroupKey) -> bool {
        self.color_ref.code() == other.color_ref.code() && self.bfc == other.bfc
    }
}
