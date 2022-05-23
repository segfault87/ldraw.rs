use std::slice::{from_raw_parts, from_raw_parts_mut};

use cgmath::{Matrix, SquareMatrix};
use ldraw::{Matrix3, Matrix4};

pub(crate) fn cast_as_bytes<T>(input: &[T]) -> &[u8] {
    unsafe { from_raw_parts(input.as_ptr() as *const u8, input.len() * 4) }
}

pub fn cast_as_bytes_mut<T>(input: &mut [T]) -> &mut [u8] {
    unsafe { from_raw_parts_mut(input.as_mut_ptr() as *mut u8, input.len() * 4) }
}

fn truncate_matrix4(m: &Matrix4) -> Matrix3 {
    Matrix3::new(
        m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2],
    )
}

pub(crate) fn derive_normal_matrix(m: &Matrix4) -> Matrix3 {
    truncate_matrix4(m)
        .invert()
        .unwrap_or_else(Matrix3::identity)
        .transpose()
}

#[cfg(test)]
mod tests {
    use ldraw::{Matrix3, Matrix4};

    use super::{derive_normal_matrix, truncate_matrix4};

    #[test]
    fn test_truncate_matrix4() {
        let matrix = Matrix4::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );

        let truncated = truncate_matrix4(&matrix);

        assert_eq!(
            truncated,
            Matrix3::new(1.0, 2.0, 3.0, 5.0, 6.0, 7.0, 9.0, 10.0, 11.0)
        )
    }

    #[test]
    fn test_derive_normal_matrix() {
        let matrix = Matrix4::new(
            1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0,
        );

        let normalized = derive_normal_matrix(&matrix);

        assert_eq!(
            normalized,
            Matrix3::new(0.5, 0.5, -0.5, 0.5, -0.5, 0.5, -0.5, 0.5, 0.5)
        )
    }
}
