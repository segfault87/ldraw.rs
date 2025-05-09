use std::{collections::HashSet, hash::Hash};

use cgmath::{prelude::*, Deg, Matrix, Ortho, PerspectiveFov, Point3, SquareMatrix};
use ldraw::{Matrix3, Matrix4, Vector2, Vector3};
use ldraw_ir::geometry::{BoundingBox2, BoundingBox3};
use wgpu::util::DeviceExt;

use crate::{AspectRatio, GpuUpdate, GpuUpdateResult};

struct ProjectionData {
    model_matrix_stack: Vec<Matrix4>,
    projection_matrix: Matrix4,
    view_matrix: Matrix4,
    is_orthographic: bool,
}

pub enum ProjectionMutator {
    PushModelMatrix(Matrix4),
    PopModelMatrix,
    SetProjectionMatrix {
        matrix: Matrix4,
        is_orthographic: bool,
    },
    SetViewMatrix(Matrix4),
}

impl ProjectionData {
    fn push_model_matrix(&mut self, mat: Matrix4) -> Matrix4 {
        let multiplied = self.model_matrix_stack.last().cloned().unwrap() * mat;
        self.model_matrix_stack.push(multiplied);
        multiplied
    }

    fn pop_model_matrix(&mut self) -> Option<Matrix4> {
        if self.model_matrix_stack.len() > 1 {
            self.model_matrix_stack.pop();
            self.model_matrix_stack.last().cloned()
        } else {
            None
        }
    }
}

impl Default for ProjectionData {
    fn default() -> Self {
        Self {
            model_matrix_stack: vec![Matrix4::identity()],
            projection_matrix: Matrix4::identity(),
            view_matrix: Matrix4::identity(),
            is_orthographic: false,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct RawProjectionData {
    model_matrix: [[f32; 4]; 4],
    projection_matrix: [[f32; 4]; 4],
    view_matrix: [[f32; 4]; 4],
    normal_matrix_0: [f32; 3],
    _padding0: [u8; 4],
    normal_matrix_1: [f32; 3],
    _padding1: [u8; 4],
    normal_matrix_2: [f32; 3],
    _padding2: [u8; 4],
    is_orthographic: i32,
    _padding3: [u8; 12],
}

impl Default for RawProjectionData {
    fn default() -> Self {
        Self {
            model_matrix: Matrix4::identity().into(),
            projection_matrix: Matrix4::identity().into(),
            view_matrix: Matrix4::identity().into(),
            normal_matrix_0: [1.0, 0.0, 0.0],
            _padding0: [0; 4],
            normal_matrix_1: [0.0, 1.0, 0.0],
            _padding1: [0; 4],
            normal_matrix_2: [0.0, 0.0, 1.0],
            _padding2: [0; 4],
            is_orthographic: 0,
            _padding3: [0; 12],
        }
    }
}

impl RawProjectionData {
    #[rustfmt::skip]
    fn truncate_matrix4(m: Matrix4) -> Matrix3 {
        Matrix3::new(
            m[0][0], m[0][1], m[0][2],
            m[1][0], m[1][1], m[1][2],
            m[2][0], m[2][1], m[2][2],
        )
    }

    fn derive_normal_matrix(m: Matrix4) -> Option<Matrix3> {
        Self::truncate_matrix4(m).invert().map(|v| v.transpose())
    }

    fn update_normal_matrix(&mut self, model_view: Matrix4) {
        let normal_matrix =
            Self::derive_normal_matrix(model_view).unwrap_or_else(Matrix3::identity);
        self.normal_matrix_0 = normal_matrix.x.into();
        self.normal_matrix_1 = normal_matrix.y.into();
        self.normal_matrix_2 = normal_matrix.z.into();
    }
}

pub struct Projection {
    pub bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: Option<wgpu::Buffer>,

    data: ProjectionData,
    raw: RawProjectionData,
}

impl GpuUpdate for Projection {
    type Mutator = ProjectionMutator;

    fn mutate(&mut self, mutator: Self::Mutator) -> GpuUpdateResult<Self::Mutator> {
        match mutator {
            ProjectionMutator::PushModelMatrix(matrix) => {
                let mat = self.data.push_model_matrix(matrix);
                self.raw.model_matrix = mat.into();
                self.raw.update_normal_matrix(self.data.view_matrix * mat);
                GpuUpdateResult::Modified
            }
            ProjectionMutator::PopModelMatrix => {
                if let Some(mat) = self.data.pop_model_matrix() {
                    self.raw.model_matrix = mat.into();
                    self.raw.update_normal_matrix(self.data.view_matrix * mat);
                    GpuUpdateResult::Modified
                } else {
                    GpuUpdateResult::NotModified
                }
            }
            ProjectionMutator::SetProjectionMatrix {
                matrix,
                is_orthographic,
            } => {
                if self.data.projection_matrix != matrix
                    || self.data.is_orthographic != is_orthographic
                {
                    self.data.projection_matrix = matrix;
                    self.data.is_orthographic = is_orthographic;
                    self.raw.projection_matrix = matrix.into();
                    self.raw.is_orthographic = if is_orthographic { 1 } else { 0 };
                    GpuUpdateResult::Modified
                } else {
                    GpuUpdateResult::NotModified
                }
            }
            ProjectionMutator::SetViewMatrix(matrix) => {
                if self.data.view_matrix != matrix {
                    self.data.view_matrix = matrix;
                    self.raw.view_matrix = matrix.into();
                    self.raw.update_normal_matrix(self.get_model_view_matrix());
                    GpuUpdateResult::Modified
                } else {
                    GpuUpdateResult::NotModified
                }
            }
        }
    }

    fn handle_gpu_update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let uniform_buffer = self.uniform_buffer.get_or_insert_with(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Uniform buffer for projection"),
                contents: bytemuck::cast_slice(&[self.raw]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });

        if self.bind_group.is_none() {
            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Bind group for projection"),
                layout: &device.create_bind_group_layout(&Self::desc()),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            }));
        }

        queue.write_buffer(
            uniform_buffer,
            0 as wgpu::BufferAddress,
            bytemuck::cast_slice(&[self.raw]),
        );
    }
}

impl Projection {
    pub fn new() -> Self {
        let data = ProjectionData::default();
        let raw = RawProjectionData::default();

        Self {
            uniform_buffer: None,
            bind_group: None,
            data,
            raw,
        }
    }

    pub fn get_model_view_matrix(&self) -> Matrix4 {
        self.data.view_matrix * self.data.model_matrix_stack.last().unwrap()
    }

    pub fn select_objects<T: Eq + PartialEq + Hash>(
        &self,
        area: &BoundingBox2,
        objects: impl Iterator<Item = (T, Matrix4, BoundingBox3)>,
    ) -> HashSet<T> {
        let mvp = self.data.projection_matrix * self.get_model_view_matrix();

        objects
            .filter_map(|(id, matrix, bb)| {
                if bb.project(&(mvp * matrix)).intersects(area) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>()
    }

    pub fn desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind group descriptor for projection"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        }
    }
}

impl Default for Projection {
    fn default() -> Self {
        Self::new()
    }
}

pub trait ProjectionModifier {
    fn update_projections(&self, aspect_ratio: AspectRatio) -> Vec<ProjectionMutator>;
}

#[derive(Clone, Debug)]
pub enum ViewBounds {
    BoundingBox3(BoundingBox3),
    BoundingBox2(BoundingBox2),
    Radius(f32),
    Unbounded,
}

impl ViewBounds {
    pub fn fraction(&self, model_view: &Matrix4) -> Option<BoundingBox2> {
        match self {
            Self::BoundingBox3(bb) => {
                let transformed_bb = {
                    let mut pbb = BoundingBox2::nil();
                    for point in bb.points() {
                        let p = model_view * point.extend(1.0);
                        pbb.update_point(&Vector2::new(p.x, p.y));
                    }
                    pbb
                };

                if transformed_bb.len_x() >= transformed_bb.len_y() {
                    let d = (transformed_bb.len_x() - transformed_bb.len_y()) * 0.5;
                    let fd = d / transformed_bb.len_x();

                    Some(BoundingBox2::new(
                        &Vector2::new(0.0, fd),
                        &Vector2::new(1.0, 1.0 - fd),
                    ))
                } else {
                    let d = (transformed_bb.len_y() - transformed_bb.len_x()) * 0.5;
                    let fd = d / transformed_bb.len_y();

                    Some(BoundingBox2::new(
                        &Vector2::new(fd, 0.0),
                        &Vector2::new(1.0 - fd, 1.0),
                    ))
                }
            }
            _ => None,
        }
    }

    pub fn project(&self, model_view: &Matrix4, aspect_ratio: AspectRatio) -> BoundingBox2 {
        let aspect_ratio: f32 = aspect_ratio.into();

        match self {
            Self::BoundingBox3(bb) => {
                let transformed_bb = {
                    let mut pbb = BoundingBox2::nil();
                    for point in bb.points() {
                        let p = model_view * point.extend(1.0);
                        pbb.update_point(&Vector2::new(p.x, p.y));
                    }
                    pbb
                };

                if transformed_bb.len_x() >= transformed_bb.len_y() {
                    let margin = transformed_bb.len_x() * 0.05;
                    let d = (transformed_bb.len_x() - transformed_bb.len_y()) * 0.5;

                    BoundingBox2::new(
                        &Vector2::new(
                            transformed_bb.min.x - margin,
                            transformed_bb.min.y - d - margin,
                        ),
                        &Vector2::new(
                            transformed_bb.max.x + margin,
                            transformed_bb.max.y + d + margin,
                        ),
                    )
                } else {
                    let margin = transformed_bb.len_x() * 0.05;
                    let d = (transformed_bb.len_y() - transformed_bb.len_x()) * 0.5;

                    BoundingBox2::new(
                        &Vector2::new(
                            transformed_bb.min.x - d - margin,
                            transformed_bb.min.y - margin,
                        ),
                        &Vector2::new(
                            transformed_bb.max.x + d + margin,
                            transformed_bb.max.y + margin,
                        ),
                    )
                }
            }
            Self::BoundingBox2(bb) => bb.clone(),
            Self::Radius(r) => BoundingBox2::new(
                &Vector2::new(-r * aspect_ratio, -r / aspect_ratio),
                &Vector2::new(r * aspect_ratio, r / aspect_ratio),
            ),
            Self::Unbounded => BoundingBox2::new(
                &Vector2::new(-300.0 * aspect_ratio, -300.0 / aspect_ratio),
                &Vector2::new(300.0 * aspect_ratio, 300.0 / aspect_ratio),
            ),
        }
    }
}

pub struct PerspectiveCamera {
    pub position: Point3<f32>,
    pub look_at: Point3<f32>,
    pub up: Vector3,
    pub fov: Deg<f32>,
}

impl PerspectiveCamera {
    pub fn new(position: Point3<f32>, look_at: Point3<f32>, fov: Deg<f32>) -> Self {
        Self {
            position,
            look_at,
            up: Vector3::new(0.0, -1.0, 0.0),
            fov,
        }
    }
}

impl ProjectionModifier for PerspectiveCamera {
    fn update_projections(&self, aspect_ratio: AspectRatio) -> Vec<ProjectionMutator> {
        let projection_matrix = Matrix4::from(PerspectiveFov {
            fovy: cgmath::Rad::from(self.fov),
            aspect: aspect_ratio.into(),
            near: 10.0,
            far: 100000.0,
        });

        let view_matrix = Matrix4::look_at_rh(self.position, self.look_at, self.up);

        vec![
            ProjectionMutator::SetProjectionMatrix {
                matrix: projection_matrix,
                is_orthographic: false,
            },
            ProjectionMutator::SetViewMatrix(view_matrix),
        ]
    }
}

#[derive(Clone, Debug)]
pub struct OrthographicCamera {
    pub position: Point3<f32>,
    pub look_at: Point3<f32>,
    pub up: Vector3,
    pub view_bounds: ViewBounds,
}

impl OrthographicCamera {
    pub fn new(position: Point3<f32>, look_at: Point3<f32>, view_bounds: ViewBounds) -> Self {
        Self {
            position,
            look_at,
            up: Vector3::new(0.0, -1.0, 0.0),
            view_bounds,
        }
    }

    pub fn new_isometric(center: Point3<f32>, view_bounds: ViewBounds) -> Self {
        let sin = Deg(45.0).sin() * 1000.0;
        let siny = Deg(35.264).sin() * 1000.0;
        let position = Point3::new(center.x + sin, center.y - siny, center.z - sin);

        Self {
            position,
            look_at: center,
            up: Vector3::new(0.0, -1.0, 0.0),
            view_bounds,
        }
    }
}

impl ProjectionModifier for OrthographicCamera {
    fn update_projections(&self, aspect_ratio: AspectRatio) -> Vec<ProjectionMutator> {
        let view_matrix = Matrix4::look_at_rh(self.position, self.look_at, self.up);
        let view_bounds = self.view_bounds.project(&view_matrix, aspect_ratio);

        let projection_matrix = Matrix4::from(Ortho {
            left: view_bounds.min.x,
            right: view_bounds.max.x,
            top: view_bounds.max.y,
            bottom: view_bounds.min.y,
            near: -10000.0,
            far: 10000.0,
        });

        vec![
            ProjectionMutator::SetProjectionMatrix {
                matrix: projection_matrix,
                is_orthographic: true,
            },
            ProjectionMutator::SetViewMatrix(view_matrix),
        ]
    }
}
