#![feature(trait_alias)]

#[macro_use] extern crate arrayref;

use glow::HasContext;
use ldraw::{Matrix3, Matrix4, Vector3};
use serde::{Deserialize, Serialize};

pub mod buffer;
pub mod display_list;
pub mod error;
pub mod geometry;
pub mod model;
pub mod shader;
pub mod state;
pub mod utils;

pub trait GL = HasContext;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoundingBox {
    pub min: Vector3,
    pub max: Vector3,
}

impl BoundingBox {
    pub fn zero() -> BoundingBox {
        BoundingBox {
            min: Vector3::new(0.0, 0.0, 0.0),
            max: Vector3::new(0.0, 0.0, 0.0),
        }
    }

    pub fn new(a: &Vector3, b: &Vector3) -> BoundingBox {
        let (min_x, max_x) = if a.x > b.x { (b.x, a.x) } else { (a.x, b.x) };
        let (min_y, max_y) = if a.y > b.y { (b.y, a.y) } else { (a.y, b.y) };
        let (min_z, max_z) = if a.z > b.z { (b.z, a.z) } else { (a.z, b.z) };

        BoundingBox {
            min: Vector3::new(min_x, min_y, min_z),
            max: Vector3::new(max_x, max_y, max_z),
        }
    }

    pub fn len_x(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn len_y(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn len_z(&self) -> f32 {
        self.max.z - self.min.z
    }

    pub fn update_point(&mut self, v: &Vector3) {
        if self.min.x > v.x {
            self.min.x = v.x;
        }
        if self.min.y > v.y {
            self.min.y = v.y;
        }
        if self.min.z > v.z {
            self.min.z = v.z;
        }
        if self.max.x < v.x {
            self.max.x = v.x;
        }
        if self.max.y < v.y {
            self.max.y = v.y;
        }
        if self.max.z < v.z {
            self.max.z = v.z;
        }
    }

    pub fn update(&mut self, bb: &BoundingBox) {
        self.update_point(&bb.min);
        self.update_point(&bb.max);
    }
}

pub fn truncate_matrix4(m: Matrix4) -> Matrix3 {
    Matrix3::new(
        m[0][0], m[0][1], m[0][2],
        m[1][0], m[1][1], m[1][2],
        m[2][0], m[2][1], m[2][2]
    )
}
