use cgmath::{Deg, Matrix, PerspectiveFov, Point3, SquareMatrix};
use ldraw::{Matrix3, Matrix4, Vector3};
use wgpu::util::DeviceExt;

#[rustfmt::skip]
fn truncate_matrix4(m: Matrix4) -> Matrix3 {
    Matrix3::new(
        m[0][0], m[0][1], m[0][2],
        m[1][0], m[1][1], m[1][2],
        m[2][0], m[2][1], m[2][2],
    )
}

fn derive_normal_matrix(m: Matrix4) -> Option<Matrix3> {
    truncate_matrix4(m).invert().map(|v| v.transpose())
}

#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: Matrix4 = cgmath::Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.5,
    0.0, 0.0, 0.0, 1.0,
);

pub struct ProjectionData {
    pub model_matrix: Vec<Matrix4>,
    pub projection_matrix: Matrix4,
    pub view_matrix: Matrix4,
    pub is_orthographic: bool,
}

impl Default for ProjectionData {
    fn default() -> Self {
        Self {
            model_matrix: vec![Matrix4::identity()],
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

impl From<&ProjectionData> for RawProjectionData {
    fn from(d: &ProjectionData) -> Self {
        let model_view = d.view_matrix * d.model_matrix.last().unwrap();
        let normal_matrix = derive_normal_matrix(model_view).unwrap_or_else(Matrix3::identity);
        Self {
            model_matrix: d
                .model_matrix
                .last()
                .cloned()
                .unwrap_or_else(Matrix4::identity)
                .into(),
            projection_matrix: d.projection_matrix.into(),
            view_matrix: d.view_matrix.into(),
            normal_matrix_0: normal_matrix.x.into(),
            _padding0: [0; 4],
            normal_matrix_1: normal_matrix.y.into(),
            _padding1: [0; 4],
            normal_matrix_2: normal_matrix.z.into(),
            _padding2: [0; 4],
            is_orthographic: if d.is_orthographic { 1 } else { 0 },
            _padding3: [0; 12],
        }
    }
}

impl RawProjectionData {
    fn update(&mut self, data: &ProjectionData) {
        if let Some(model_matrix) = data.model_matrix.last() {
            self.model_matrix = (*model_matrix).into();
        }
        self.projection_matrix = data.projection_matrix.into();
        self.view_matrix = data.view_matrix.into();
        let model_view = data.view_matrix * data.model_matrix.last().unwrap();
        let normal_matrix = derive_normal_matrix(model_view).unwrap_or_else(Matrix3::identity);
        self.normal_matrix_0 = normal_matrix.x.into();
        self.normal_matrix_1 = normal_matrix.y.into();
        self.normal_matrix_2 = normal_matrix.z.into();
        self.is_orthographic = if data.is_orthographic { 1 } else { 0 };
    }
}

pub struct Projection {
    pub bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,

    pub data: ProjectionData,
    raw: RawProjectionData,
}

impl Projection {
    pub fn new(device: &wgpu::Device) -> Self {
        let data = ProjectionData::default();
        let raw = RawProjectionData::from(&data);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform buffer for projection"),
            contents: bytemuck::cast_slice(&[raw]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind group for projection"),
            layout: &device.create_bind_group_layout(&Self::desc()),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            uniform_buffer,
            bind_group,
            data,
            raw,
        }
    }

    pub fn update_camera(
        &mut self,
        queue: &wgpu::Queue,
        camera: &impl ProjectionModifier,
        width: u32,
        height: u32,
    ) {
        if camera.update_projections(&mut self.data, width, height) {
            self.update_buffer(queue);
        }
    }

    pub fn push_model_matrix(&mut self, matrix: Matrix4) {
        let last = self.data.model_matrix.last().unwrap();
        self.data.model_matrix.push(last * matrix);
    }

    pub fn pop_model_matrix(&mut self) -> Option<Matrix4> {
        if self.data.model_matrix.len() > 1 {
            self.data.model_matrix.pop()
        } else {
            None
        }
    }

    pub fn update_buffer(&mut self, queue: &wgpu::Queue) {
        self.raw.update(&self.data);

        queue.write_buffer(
            &self.uniform_buffer,
            0 as wgpu::BufferAddress,
            bytemuck::cast_slice(&[self.raw]),
        );
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

pub trait ProjectionModifier {
    fn update_projections(&self, projection: &mut ProjectionData, width: u32, height: u32) -> bool;
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
    fn update_projections(&self, projection: &mut ProjectionData, width: u32, height: u32) -> bool {
        let aspect_ratio = width as f32 / height as f32;

        projection.projection_matrix = Matrix4::from(PerspectiveFov {
            fovy: cgmath::Rad::from(self.fov),
            aspect: aspect_ratio,
            near: 10.0,
            far: 100000.0,
        });
        projection.view_matrix = Matrix4::look_at_rh(self.position, self.look_at, self.up);

        projection.is_orthographic = false;

        true
    }
}
