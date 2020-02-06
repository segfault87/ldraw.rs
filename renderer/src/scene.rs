use cgmath::{InnerSpace, Matrix, SquareMatrix};
use ldraw::{Matrix3, Matrix4, Vector4};

pub struct ProjectionParams {
    pub projection: Matrix4,
    pub model_view: Matrix4,
    pub view_matrix: Matrix4,
}

fn matrix3_from_matrix4(m: Matrix4) -> Matrix3 {
    Matrix3::new(
        m[0][0], m[0][1], m[0][2],
        m[1][0], m[1][1], m[1][2],
        m[2][0], m[2][1], m[2][2]
    )
}

impl ProjectionParams {
    pub fn new() -> ProjectionParams {
        ProjectionParams {
            projection: Matrix4::identity(),
            model_view: Matrix4::identity(),
            view_matrix: Matrix4::identity(),
        }
    }

    pub fn calculate_normal_matrix(&self) -> Matrix3 {
        matrix3_from_matrix4(
            self.model_view.invert().unwrap_or(Matrix4::identity()).transpose()
        )
    }

    pub fn calculate_normal_matrix_with(&self, m: &Matrix4) -> Matrix3 {
        matrix3_from_matrix4(
            (self.model_view * m).invert().unwrap_or(Matrix4::identity()).transpose()
        )    
    }
}

impl Default for ProjectionParams {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ShadingParams {
    pub light_color: Vector4,
    pub light_direction: Vector4,
}

impl ShadingParams {
    pub fn new() -> ShadingParams {
        ShadingParams {
            light_color: Vector4::new(1.0, 1.0, 1.0, 1.0),
            light_direction: Vector4::new(-0.2, 0.45, 0.5, 1.0).normalize(),
        }
    }
}

impl Default for ShadingParams {
    fn default() -> Self {
        Self::new()
    }
}
