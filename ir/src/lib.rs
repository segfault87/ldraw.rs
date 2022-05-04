use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    mem::replace,
};

use ldraw::color::{ColorReference, MaterialRegistry};
use serde::{Deserialize, Serialize};

pub mod constraints;
pub mod document;
pub mod geometry;
pub mod part;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MeshGroup {
    pub color_ref: ColorReference,
    pub bfc: bool,
}

impl MeshGroup {
    pub fn resolve_color(&mut self, materials: &MaterialRegistry) {
        self.color_ref.resolve_self(materials);
    }
}

impl Hash for MeshGroup {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color_ref.code().hash(state);
        self.bfc.hash(state);
    }
}

impl PartialOrd for MeshGroup {
    fn partial_cmp(&self, other: &MeshGroup) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MeshGroup {
    fn cmp(&self, other: &MeshGroup) -> Ordering {
        let lhs_translucent = match &self.color_ref {
            ColorReference::Material(m) => m.is_translucent(),
            _ => false,
        };
        let rhs_translucent = match &other.color_ref {
            ColorReference::Material(m) => m.is_translucent(),
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

impl Eq for MeshGroup {}

impl PartialEq for MeshGroup {
    fn eq(&self, other: &MeshGroup) -> bool {
        self.color_ref.code() == other.color_ref.code() && self.bfc == other.bfc
    }
}
