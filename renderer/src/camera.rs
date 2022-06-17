use cgmath::{Angle, Deg, Ortho, PerspectiveFov, Point3, Rad, SquareMatrix};
use ldraw::{Matrix4, Vector2, Vector3};
use ldraw_ir::geometry::{BoundingBox2, BoundingBox3};

pub trait Camera {
    fn get_projection_matrix(&self, aspect_ratio: f32) -> Matrix4;
    fn get_view_matrix(&self) -> Matrix4;
    fn is_orthographic(&self) -> bool;
}

pub static VIEW_FRUSTUM_NEAR: f32 = 10.0;
pub static VIEW_FRUSTUM_FAR: f32 = 100000.0;

pub struct PerspectiveCamera {
    pub position: Point3<f32>,
    pub look_at: Point3<f32>,
    pub up: Vector3,
    pub fov: Deg<f32>,
}

fn transform_bounding_box_3d(bounding_box: &BoundingBox3, matrix: &Matrix4) -> BoundingBox2 {
    let mut pbb = BoundingBox2::zero();
    for point in bounding_box.points() {
        let p = matrix * point.extend(1.0);
        pbb.update_point(&Vector2::new(p.x, p.y));
    }

    pbb
}

impl PerspectiveCamera {
    pub fn new(position: Point3<f32>, look_at: Point3<f32>, fov: Deg<f32>) -> Self {
        PerspectiveCamera {
            position,
            look_at,
            up: Vector3::new(0.0, -1.0, 0.0),
            fov,
        }
    }
}

impl Camera for PerspectiveCamera {
    fn get_projection_matrix(&self, aspect_ratio: f32) -> Matrix4 {
        Matrix4::from(PerspectiveFov {
            fovy: Rad::from(self.fov),
            aspect: aspect_ratio,
            near: VIEW_FRUSTUM_NEAR,
            far: VIEW_FRUSTUM_FAR,
        })
    }

    fn get_view_matrix(&self) -> Matrix4 {
        Matrix4::look_at_rh(self.position, self.look_at, self.up)
    }

    fn is_orthographic(&self) -> bool {
        false
    }
}

pub struct OrthographicCamera {
    pub position: Point3<f32>,
    pub look_at: Point3<f32>,
    pub up: Vector3,
    pub size_multiplier: f32,
}

#[derive(Copy, Clone)]
pub enum ContentPadding {
    Multiplier(f32),
    Fixed(f32),
    None,
}

impl ContentPadding {
    pub fn calculate(&self, value: f32) -> f32 {
        match self {
            ContentPadding::Multiplier(v) => value * v,
            ContentPadding::Fixed(v) => *v,
            ContentPadding::None => 0.0,
        }
    }
}

impl OrthographicCamera {
    pub fn new(
        position: Point3<f32>,
        look_at: Point3<f32>,
        size_multiplier: f32,
        aspect_ratio: f32,
    ) -> Self {
        OrthographicCamera {
            position,
            look_at,
            up: Vector3::new(0.0, -1.0, 0.0),
            size_multiplier,
        }
    }

    pub fn fit_bounding_box_3d(
        &mut self,
        bounding_box: &BoundingBox3,
        padding: ContentPadding,
        model_matrix: Option<Matrix4>,
    ) {
        let model_matrix = model_matrix.unwrap_or_else(Matrix4::identity);
        let view_matrix = self.get_view_matrix();

        let projected_bb = transform_bounding_box_3d(bounding_box, &(view_matrix * model_matrix));

        let xlen = projected_bb.len_x();
        let ylen = projected_bb.len_y();

        let adjusted = if xlen >= ylen {
            let margin = padding.calculate(xlen);
            let d = (xlen - ylen) * 0.5;

            BoundingBox2::new(
                &Vector2::new(projected_bb.min.x - margin, projected_bb.min.y - d - margin),
                &Vector2::new(projected_bb.max.x + margin, projected_bb.max.y + d + margin),
            )
        } else {
            let margin = padding.calculate(ylen);
            let d = (ylen - xlen) * 0.5;

            BoundingBox2::new(
                &Vector2::new(projected_bb.min.x - d - margin, projected_bb.min.y - margin),
                &Vector2::new(projected_bb.max.x + d + margin, projected_bb.max.y + margin),
            )
        };
    }

    pub fn fit_bounding_box_2d(&mut self, bounding_box: &BoundingBox2) {}
}

impl Camera for OrthographicCamera {
    fn get_projection_matrix(&self, aspect_ratio: f32) -> Matrix4 {
        let xs = self.size_multiplier;
        let ys = self.size_multiplier * aspect_ratio;

        Matrix4::from(Ortho {
            left: -xs,
            right: xs,
            top: -ys,
            bottom: ys,
            near: VIEW_FRUSTUM_NEAR,
            far: VIEW_FRUSTUM_FAR,
        })
    }

    fn get_view_matrix(&self) -> Matrix4 {
        Matrix4::look_at_rh(self.position, self.look_at, self.up)
    }

    fn is_orthographic(&self) -> bool {
        true
    }
}
