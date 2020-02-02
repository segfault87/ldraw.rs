use cgmath::{InnerSpace, Matrix, SquareMatrix};
use ldraw::{Matrix3, Matrix4, Vector4};

pub struct ProjectionParams {
    pub projection: Matrix4,
    pub model_view: Matrix4,
    pub view_matrix: Matrix4,
}

impl ProjectionParams {
    pub fn new() -> ProjectionParams {
        ProjectionParams {
            projection: Matrix4::identity(),
            model_view: Matrix4::identity(),
            view_matrix: Matrix4::identity(),
        }
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
            light_direction: Vector4::new(0.0, -0.5, 0.7, 1.0).normalize(),
        }
    }
}

impl Default for ShadingParams {
    fn default() -> Self {
        Self::new()
    }
}
