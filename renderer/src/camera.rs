use cgmath::SquareMatrix;
use ldraw::{Matrix3, Matrix4};
use wgpu::util::DeviceExt;

pub struct ProjectionData {
    pub model_matrix: Vec<Matrix4>,
    pub projection_matrix: Matrix4,
    pub model_view_matrix: Matrix4,
    pub normal_matrix: Matrix3,
    pub view_matrix: Matrix4,
    pub is_orthographic: bool,
}

impl Default for ProjectionData {
    fn default() -> Self {
        Self {
            model_matrix: vec![Matrix4::identity()],
            projection_matrix: Matrix4::identity(),
            model_view_matrix: Matrix4::identity(),
            normal_matrix: Matrix3::identity(),
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
    model_view_matrix: [[f32; 4]; 4],
    normal_matrix: [[f32; 3]; 3],
    view_matrix: [[f32; 4]; 4],
    is_orthographic: i32,
    _padding: [u8; 24],
}

impl From<&ProjectionData> for RawProjectionData {
    fn from(d: &ProjectionData) -> Self {
        Self {
            model_matrix: d
                .model_matrix
                .last()
                .cloned()
                .unwrap_or_else(Matrix4::identity)
                .into(),
            projection_matrix: d.projection_matrix.into(),
            model_view_matrix: d.model_view_matrix.into(),
            normal_matrix: d.normal_matrix.into(),
            view_matrix: d.view_matrix.into(),
            is_orthographic: if d.is_orthographic { 1 } else { 0 },
            _padding: [0; 24],
        }
    }
}

impl RawProjectionData {
    pub fn update(&mut self, data: &ProjectionData) {
        if let Some(model_matrix) = data.model_matrix.last() {
            self.model_matrix = (*model_matrix).into();
        }
        self.projection_matrix = data.projection_matrix.into();
        self.model_view_matrix = data.model_view_matrix.into();
        self.normal_matrix = data.normal_matrix.into();
        self.view_matrix = data.view_matrix.into();
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

    pub fn update(&mut self, queue: &wgpu::Queue) {
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
                visibility: wgpu::ShaderStages::VERTEX,
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
