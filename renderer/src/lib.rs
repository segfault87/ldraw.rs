#![feature(array_map)]

use ldraw::{Matrix3, Matrix4};

pub mod display_list;
pub mod error;
pub mod model;
pub mod part;
pub mod shader;
pub mod state;
pub mod utils;

pub fn truncate_matrix4(m: Matrix4) -> Matrix3 {
    Matrix3::new(
        m[0][0], m[0][1], m[0][2],
        m[1][0], m[1][1], m[1][2],
        m[2][0], m[2][1], m[2][2]
    )
}
