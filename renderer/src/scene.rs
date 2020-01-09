use cgmath::{InnerSpace, Matrix, SquareMatrix};
use ldraw::{Matrix3, Matrix4, Vector4};

pub struct ProjectionParams {
    pub projection: Matrix4,
    pub model_view: Matrix4,
    pub view_matrix: Matrix4,
    view_inverted: Matrix4,
    normal_matrix: Matrix3,
}

impl ProjectionParams {
    pub fn new() -> ProjectionParams {
        ProjectionParams {
            projection: Matrix4::identity(),
            model_view: Matrix4::identity(),
            view_matrix: Matrix4::identity(),
            view_inverted: Matrix4::identity(),
            normal_matrix: Matrix3::identity(),
        }
    }

    fn inv_mat3(src: &Matrix4) -> Matrix3 {
        let a00 = src[0][0];
        let a01 = src[0][1];
        let a02 = src[0][2];
        let a10 = src[1][0];
        let a11 = src[1][1];
        let a12 = src[1][2];
        let a20 = src[2][0];
        let a21 = src[2][1];
        let a22 = src[2][2];

        let b01 = a22 * a11 - a12 * a21;
        let b11 = -a22 * a10 + a12 * a20;
        let b21 = a21 * a10 - a11 * a20;

        let det = a00 * b01 + a01 * b11 + a02 * b21;
        if det == 0.0 {
            panic!("Could not invert this matrix.");
        }
        let id = 1.0 / det;

        Matrix3::new(
            b01 * id,
            (-a22 * a01 + a02 * a21) * id,
            (a12 * a01 - a02 * a11) * id,
            b11 * id,
            (a22 * a00 - a02 * a20) * id,
            (-a12 * a00 + a02 * a10) * id,
            b21 * id,
            (-a21 * a00 + a01 * a20) * id,
            (a11 * a00 - a01 * a10) * id,
        )
    }

    pub fn update(&mut self) {
        self.view_inverted = self.view_matrix.invert().unwrap();
        self.normal_matrix = Self::inv_mat3(&self.model_view).transpose();
    }

    pub fn get_inverted_view_matrix(&self) -> &Matrix4 {
        &self.view_inverted
    }

    pub fn get_normal_matrix(&self) -> &Matrix3 {
        &self.normal_matrix
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
