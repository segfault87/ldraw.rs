use std::{
    slice::from_raw_parts,
};

use cgmath::{Matrix, SquareMatrix};
use ldraw::{Matrix3, Matrix4};

pub(crate) fn cast_as_bytes<'a>(input: &'a [f32]) -> &'a [u8] {
    unsafe { from_raw_parts(input.as_ptr() as *const u8, input.len() * 4) }
}

fn truncate_matrix4(m: &Matrix4) -> Matrix3 {
    Matrix3::new(
        m[0][0], m[0][1], m[0][2],
        m[1][0], m[1][1], m[1][2],
        m[2][0], m[2][1], m[2][2]
    )
}

pub(crate) fn derive_normal_matrix(m: &Matrix4) -> Matrix3 {
    truncate_matrix4(m).invert().unwrap_or(Matrix3::identity()).transpose()
}
