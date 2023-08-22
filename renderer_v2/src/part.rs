use std::{collections::HashMap, ops::Range};

use ldraw::{color::ColorCatalog, PartAlias, Vector3};
use ldraw_ir::{geometry::BoundingBox3, part as part_ir, MeshGroupKey};
use wgpu::util::DeviceExt;

pub struct MeshBuffer {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,

    pub uncolored_range: Option<Range<u32>>,
    pub uncolored_without_bfc_range: Option<Range<u32>>,
    pub opaque_ranges: HashMap<MeshGroupKey, Range<u32>>,
    pub translucent_ranges: HashMap<MeshGroupKey, Range<u32>>,
}

#[derive(Eq, PartialEq, Hash)]
struct MeshVertexIndex {
    vertex: usize,
    normal: usize,
}

impl MeshBuffer {
    fn expand(
        metadata: &part_ir::PartMetadata,
        vertices: &mut Vec<f32>,
        index: &mut Vec<u32>,
        index_table: &mut HashMap<MeshVertexIndex, u32>,
        vertex_buffer: &part_ir::VertexBuffer,
        index_buffer: &part_ir::MeshBuffer,
    ) -> Option<Range<u32>> {
        if index_buffer.is_empty() {
            None
        } else if !index_buffer.is_valid() {
            eprintln!(
                "{}: Corrupted mesh vertex buffer. skipping...",
                metadata.name
            );
            None
        } else {
            let start = index.len() as u32;
            let end = start + index_buffer.len() as u32;

            for (vertex_idx, normal_idx) in index_buffer
                .vertex_indices
                .iter()
                .zip(index_buffer.normal_indices.iter())
            {
                let vertex_idx = *vertex_idx as usize;
                let normal_idx = *normal_idx as usize;

                let vertex_range = vertex_idx * 3..vertex_idx * 3 + 3;
                let normal_range = normal_idx * 3..normal_idx * 3 + 3;

                if !vertex_buffer.check_range(&vertex_range)
                    || !vertex_buffer.check_range(&normal_range)
                {
                    eprintln!(
                        "{}: Corrupted mesh vertex buffer. skipping...",
                        metadata.name
                    );
                    return None;
                }

                let idx_key = MeshVertexIndex {
                    vertex: vertex_idx,
                    normal: normal_idx,
                };

                if let Some(idx) = index_table.get(&idx_key) {
                    index.push(*idx);
                } else {
                    let idx_val = index_table.len() as u32;
                    index_table.insert(idx_key, idx_val);
                    vertices.extend(&vertex_buffer.0[vertex_range]);
                    vertices.extend(&vertex_buffer.0[normal_range]);
                    index.push(idx_val);
                }
            }

            if start == end {
                None
            } else {
                Some(start..end)
            }
        }
    }

    pub fn new(device: &wgpu::Device, part: &part_ir::Part) -> Self {
        let mut data = Vec::new();
        let mut index = Vec::new();
        let mut index_lut = HashMap::new();

        let uncolored_range = Self::expand(
            &part.metadata,
            &mut data,
            &mut index,
            &mut index_lut,
            &part.geometry.vertex_buffer,
            &part.geometry.uncolored_mesh,
        );
        let uncolored_without_bfc_range = Self::expand(
            &part.metadata,
            &mut data,
            &mut index,
            &mut index_lut,
            &part.geometry.vertex_buffer,
            &part.geometry.uncolored_without_bfc_mesh,
        );
        let opaque_ranges = part
            .geometry
            .opaque_meshes
            .iter()
            .filter_map(|(k, v)| {
                if let Some(v) = Self::expand(
                    &part.metadata,
                    &mut data,
                    &mut index,
                    &mut index_lut,
                    &part.geometry.vertex_buffer,
                    v,
                ) {
                    Some((k.clone(), v))
                } else {
                    None
                }
            })
            .collect();
        let translucent_ranges = part
            .geometry
            .translucent_meshes
            .iter()
            .filter_map(|(k, v)| {
                if let Some(v) = Self::expand(
                    &part.metadata,
                    &mut data,
                    &mut index,
                    &mut index_lut,
                    &part.geometry.vertex_buffer,
                    v,
                ) {
                    Some((k.clone(), v))
                } else {
                    None
                }
            })
            .collect();

        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!(
                "Vertex buffer for mesh data at {}",
                part.metadata.name
            )),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!(
                "Index buffer for mesh data at {}",
                part.metadata.name
            )),
            contents: bytemuck::cast_slice(&index),
            usage: wgpu::BufferUsages::INDEX,
        });

        println!(
            "{}: {} bytes for mesh buffer",
            part.metadata.name,
            data.len() * 4
        );

        MeshBuffer {
            vertices,
            indices,
            uncolored_range,
            uncolored_without_bfc_range,
            opaque_ranges,
            translucent_ranges,
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[derive(Eq, PartialEq, Hash)]
struct EdgeVertexIndex {
    vertex: usize,
    color: u32,
}

pub struct EdgeBuffer {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,

    pub range: Range<u32>,
}

impl EdgeBuffer {
    fn expand(
        metadata: &part_ir::PartMetadata,
        vertices: &mut Vec<f32>,
        index: &mut Vec<u32>,
        index_table: &mut HashMap<EdgeVertexIndex, u32>,
        colors: &ColorCatalog,
        vertex_buffer: &part_ir::VertexBuffer,
        index_buffer: &part_ir::EdgeBuffer,
    ) -> Option<Range<u32>> {
        if index_buffer.is_empty() {
            None
        } else if !index_buffer.is_valid() {
            eprintln!("{}: Corrupted edge buffer. skipping...", metadata.name);
            None
        } else {
            let start = index.len() as u32;
            let end = start + index_buffer.len() as u32;

            for (vertex_idx, color_id) in index_buffer
                .vertex_indices
                .iter()
                .zip(index_buffer.colors.iter())
            {
                let vertex_idx = *vertex_idx as usize;
                let vertex_range = vertex_idx * 3..vertex_idx * 3 + 3;
                if !vertex_buffer.check_range(&vertex_range) {
                    eprintln!("{}: Corrupted edge buffer. skipping...", metadata.name);
                    return None;
                }

                let idx_key = EdgeVertexIndex {
                    vertex: vertex_idx,
                    color: *color_id,
                };

                if let Some(idx) = index_table.get(&idx_key) {
                    index.push(*idx);
                } else {
                    let color = if *color_id == 2u32 << 30 {
                        [-1.0, -1.0, -1.0]
                    } else if *color_id == 2u32 << 29 {
                        [-2.0, -2.0, -2.0]
                    } else {
                        match colors.get(&(color_id & 0x7fffffffu32)) {
                            Some(color) => {
                                let buf = if *color_id & 0x8000_0000 != 0 {
                                    &color.edge
                                } else {
                                    &color.color
                                };

                                let r = buf.red() as f32 / 255.0;
                                let g = buf.green() as f32 / 255.0;
                                let b = buf.blue() as f32 / 255.0;

                                [r, g, b]
                            }
                            None => [0.0, 0.0, 0.0],
                        }
                    };

                    let idx_val = index_table.len() as u32;
                    index_table.insert(idx_key, idx_val);
                    vertices.extend(&vertex_buffer.0[vertex_range]);
                    vertices.extend(&color);
                    index.push(idx_val);
                }
            }

            if start == end {
                None
            } else {
                Some(start..end)
            }
        }
    }

    pub fn new(device: &wgpu::Device, colors: &ColorCatalog, part: &part_ir::Part) -> Option<Self> {
        let mut data = Vec::new();
        let mut index = Vec::new();
        let mut index_lut = HashMap::new();

        if let Some(range) = Self::expand(
            &part.metadata,
            &mut data,
            &mut index,
            &mut index_lut,
            colors,
            &part.geometry.vertex_buffer,
            &part.geometry.edges,
        ) {
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!(
                    "Vertex buffer for edge data at {}",
                    part.metadata.name
                )),
                contents: bytemuck::cast_slice(&data),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!(
                    "Index buffer for edge data at {}",
                    part.metadata.name
                )),
                contents: bytemuck::cast_slice(&index),
                usage: wgpu::BufferUsages::INDEX,
            });

            Some(Self {
                vertices,
                indices,
                range,
            })
        } else {
            None
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

pub struct OptionalEdgeBuffer {
    pub vertices: wgpu::Buffer,
    pub range: Range<u32>,
}

impl OptionalEdgeBuffer {
    fn expand(
        metadata: &part_ir::PartMetadata,
        vertices: &mut Vec<f32>,
        colors: &ColorCatalog,
        vertex_buffer: &part_ir::VertexBuffer,
        index_buffer: &part_ir::OptionalEdgeBuffer,
    ) -> Option<Range<u32>> {
        if index_buffer.is_empty() {
            None
        } else if !index_buffer.is_valid() {
            eprintln!(
                "{}: Corrupted optional edge buffer. skipping...",
                metadata.name
            );
            None
        } else {
            let start = vertices.len() as u32 / 3;
            let end = start + index_buffer.len() as u32;

            for i in 0..index_buffer.vertex_indices.len() {
                let vertex_idx = index_buffer.vertex_indices[i] as usize;
                let control_1_idx = index_buffer.control_1_indices[i] as usize;
                let control_2_idx = index_buffer.control_2_indices[i] as usize;
                let direction_idx = index_buffer.direction_indices[i] as usize;
                let color_id = index_buffer.colors[i] as u32;

                let vertex_range = vertex_idx * 3..vertex_idx * 3 + 3;
                let control_1_range = control_1_idx * 3..control_1_idx * 3 + 3;
                let control_2_range = control_2_idx * 3..control_2_idx * 3 + 3;
                let direction_range = direction_idx * 3..direction_idx * 3 + 3;

                if !vertex_buffer.check_range(&vertex_range)
                    || !vertex_buffer.check_range(&control_1_range)
                    || !vertex_buffer.check_range(&control_2_range)
                    || !vertex_buffer.check_range(&direction_range)
                {
                    eprintln!(
                        "{}: Corrupted optional edge buffer. skipping...",
                        metadata.name
                    );
                    return None;
                }

                let color = if color_id == 2u32 << 30 {
                    [-1.0, -1.0, -1.0]
                } else if color_id == 2u32 << 29 {
                    [-2.0, -2.0, -2.0]
                } else {
                    match colors.get(&(color_id & 0x7fffffffu32)) {
                        Some(color) => {
                            let buf = if color_id & 0x8000_0000 != 0 {
                                &color.edge
                            } else {
                                &color.color
                            };

                            let r = buf.red() as f32 / 255.0;
                            let g = buf.green() as f32 / 255.0;
                            let b = buf.blue() as f32 / 255.0;

                            [r, g, b]
                        }
                        None => [0.0, 0.0, 0.0],
                    }
                };

                vertices.extend(&vertex_buffer.0[vertex_range]);
                vertices.extend(&vertex_buffer.0[control_1_range]);
                vertices.extend(&vertex_buffer.0[control_2_range]);
                vertices.extend(&vertex_buffer.0[direction_range]);
                vertices.extend(&color);
            }

            if start == end {
                None
            } else {
                Some(start..end)
            }
        }
    }

    pub fn new(device: &wgpu::Device, colors: &ColorCatalog, part: &part_ir::Part) -> Option<Self> {
        let mut data = Vec::new();

        if let Some(range) = Self::expand(
            &part.metadata,
            &mut data,
            colors,
            &part.geometry.vertex_buffer,
            &part.geometry.optional_edges,
        ) {
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!(
                    "Vertex buffer for optional edge data at {}",
                    part.metadata.name
                )),
                contents: bytemuck::cast_slice(&data),
                usage: wgpu::BufferUsages::VERTEX,
            });

            Some(Self { vertices, range })
        } else {
            None
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 15]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 9]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

pub struct Part {
    pub metadata: part_ir::PartMetadata,
    pub mesh: MeshBuffer,
    pub edges: Option<EdgeBuffer>,
    pub optional_edges: Option<OptionalEdgeBuffer>,
}

impl Part {
    pub fn new(device: &wgpu::Device, colors: &ColorCatalog, part: &part_ir::Part) -> Self {
        Self {
            metadata: part.metadata.clone(),
            mesh: MeshBuffer::new(device, part),
            edges: EdgeBuffer::new(device, colors, part),
            optional_edges: OptionalEdgeBuffer::new(device, colors, part),
        }
    }
}
