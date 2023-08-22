use std::{
    collections::HashMap,
    hash::Hash,
    mem,
    ops::{Range, RangeInclusive},
};

use ldraw::{Matrix4, PartAlias, Vector4};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    model_matrix: [[f32; 4]; 4],
    color: [f32; 4],
    edge_color: [f32; 4],
}

pub struct Instances<K> {
    alias: PartAlias,
    index: HashMap<K, usize>,
    instance_data: Vec<InstanceData>,

    pub instance_buffer: wgpu::Buffer,
}

impl<K> Instances<K> {
    pub fn count(&self) -> usize {
        self.instance_data.len()
    }

    pub fn range(&self) -> Range<u32> {
        0..self.count() as u32
    }

    fn update_buffer_partial(&self, queue: &wgpu::Queue, range: RangeInclusive<usize>) {
        queue.write_buffer(
            &self.instance_buffer,
            (range.start() * mem::size_of::<InstanceData>()) as wgpu::BufferAddress,
            &bytemuck::cast_slice(&self.instance_data[range]),
        )
    }

    fn rebuild_buffer(&mut self, device: &wgpu::Device) {
        self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Instance buffer for {}", self.alias)),
            contents: bytemuck::cast_slice(&self.instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 12,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 13,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 16]>() as wgpu::BufferAddress,
                    shader_location: 14,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 20]>() as wgpu::BufferAddress,
                    shader_location: 15,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

impl<K: Clone + Eq + PartialEq + Hash> Instances<K> {
    pub fn new(device: &wgpu::Device, alias: PartAlias) -> Self {
        let instance_data = Vec::new();
        let index = HashMap::new();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Instance buffer for {}", alias)),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            alias,
            index,
            instance_data,

            instance_buffer,
        }
    }

    pub fn modify(&mut self) -> InstanceTransaction<K> {
        InstanceTransaction {
            instances: self,
            ops: Vec::new(),
        }
    }
}

enum Ops<K> {
    Insert {
        key: K,
        matrix: Matrix4,
        color: Vector4,
        edge_color: Vector4,
    },
    Update {
        key: K,
        matrix: Matrix4,
        color: Vector4,
        edge_color: Vector4,
    },
    UpdateMatrix {
        key: K,
        matrix: Matrix4,
    },
    UpdateColor {
        key: K,
        color: Vector4,
        edge_color: Vector4,
    },
    Remove(K),
}

pub struct InstanceTransaction<'a, K> {
    instances: &'a mut Instances<K>,
    ops: Vec<Ops<K>>,
}

impl<'a, K: Clone + Eq + PartialEq + Hash> InstanceTransaction<'a, K> {
    pub fn insert(&mut self, key: K, matrix: Matrix4, color: Vector4, edge_color: Vector4) {
        self.ops.push(Ops::Insert {
            key,
            matrix,
            color,
            edge_color,
        });
    }

    pub fn update(&mut self, key: K, matrix: Matrix4, color: Vector4, edge_color: Vector4) {
        self.ops.push(Ops::Update {
            key,
            matrix,
            color,
            edge_color,
        });
    }

    pub fn update_matrix(&mut self, key: K, matrix: Matrix4) {
        self.ops.push(Ops::UpdateMatrix { key, matrix });
    }

    pub fn update_color(&mut self, key: K, color: Vector4, edge_color: Vector4) {
        self.ops.push(Ops::UpdateColor {
            key,
            color,
            edge_color,
        });
    }

    pub fn remove(&mut self, key: K) {
        self.ops.push(Ops::Remove(key));
    }

    pub fn commit(mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut rows_to_remove = vec![];
        let mut rows_to_insert = HashMap::new();
        let mut changed_indices = vec![];

        let instances = &mut self.instances;

        for op in self.ops {
            match op {
                Ops::Insert {
                    key,
                    matrix,
                    color,
                    edge_color,
                } => {
                    rows_to_insert.insert(key, (matrix, color, edge_color));
                }
                Ops::Remove(key) => rows_to_remove.push(key),
                Ops::Update {
                    key,
                    matrix,
                    color,
                    edge_color,
                } => {
                    if let Some(entry_idx) = instances.index.get(&key).cloned() {
                        let data = &mut instances.instance_data[entry_idx];
                        data.model_matrix = matrix.into();
                        data.color = color.into();
                        data.edge_color = edge_color.into();
                        changed_indices.push(entry_idx);
                    } else if let Some(entry) = rows_to_insert.get_mut(&key) {
                        entry.0 = matrix.into();
                        entry.1 = color.into();
                        entry.2 = edge_color.into();
                    }
                }
                Ops::UpdateMatrix { key, matrix } => {
                    if let Some(entry_idx) = instances.index.get(&key).cloned() {
                        let data = &mut instances.instance_data[entry_idx];
                        data.model_matrix = matrix.into();
                        changed_indices.push(entry_idx);
                    } else if let Some(entry) = rows_to_insert.get_mut(&key) {
                        entry.0 = matrix.into();
                    }
                }
                Ops::UpdateColor {
                    key,
                    color,
                    edge_color,
                } => {
                    if let Some(entry_idx) = instances.index.get(&key).cloned() {
                        let data = &mut instances.instance_data[entry_idx];
                        data.color = color.into();
                        data.edge_color = edge_color.into();
                        changed_indices.push(entry_idx);
                    } else if let Some(entry) = rows_to_insert.get_mut(&key) {
                        entry.1 = color.into();
                        entry.2 = edge_color.into();
                    }
                }
            }
        }

        let mut layout_changed = false;

        let mut rows_to_remove = rows_to_remove
            .into_iter()
            .filter_map(|key| instances.index.get(&key).map(|v| (key, *v)))
            .collect::<Vec<_>>();
        rows_to_remove.sort_by_key(|v| std::cmp::Reverse(v.1));

        for (key, (matrix, color, edge_color)) in rows_to_insert.into_iter() {
            if let Some((old_key, idx_to_reuse)) = rows_to_remove.pop() {
                // Take over removed rows and fill with inserted ones if available
                let data = &mut instances.instance_data[idx_to_reuse];
                data.model_matrix = matrix.into();
                data.color = color.into();
                data.edge_color = edge_color.into();
                instances.index.remove(&old_key);
                instances.index.insert(key, idx_to_reuse);
                changed_indices.push(idx_to_reuse);
            } else {
                // Insert new rows
                layout_changed = true;
                instances.instance_data.push(InstanceData {
                    model_matrix: matrix.into(),
                    color: color.into(),
                    edge_color: edge_color.into(),
                });
                instances
                    .index
                    .insert(key, instances.instance_data.len() - 1);
            }
        }

        // Remove rows
        if !rows_to_remove.is_empty() {
            rows_to_remove.reverse();

            let len = instances.instance_data.len();

            let mut removed = 0;
            for (key, index) in rows_to_remove.iter() {
                instances.index.remove(key);
                instances.instance_data.remove(index - removed);
                removed += 1;
                layout_changed = true;
            }

            // Squash the index
            removed = 0;
            let mut next_index = 0;
            let reverse_lookup = instances
                .index
                .clone()
                .into_iter()
                .map(|(k, v)| (v, k))
                .collect::<HashMap<_, _>>();
            for i in 0..len {
                if removed > 0 {
                    if let Some(k) = reverse_lookup.get(&i) {
                        if let Some(v) = instances.index.get_mut(k) {
                            *v -= removed;
                        }
                    }
                }
                if rows_to_remove.len() < next_index && rows_to_remove[next_index].1 == i {
                    next_index += 1;
                    removed += 1;
                }
            }
        }

        if layout_changed {
            instances.rebuild_buffer(device);
        } else if !changed_indices.is_empty() {
            changed_indices.sort();
            let mut start = changed_indices[0] as usize;
            let mut end = start;
            for index in changed_indices {
                if index > end + 1 {
                    instances.update_buffer_partial(queue, start..=end);
                    start = index;
                    end = index;
                } else {
                    end = index;
                }
            }
            instances.update_buffer_partial(queue, start..=end);
        }
    }
}

pub struct DisplayList<K> {
    map: HashMap<PartAlias, Instances<K>>,
}

impl<K: Clone + Eq + PartialEq + Hash> DisplayList<K> {
    pub fn get(&self, alias: &PartAlias) -> Option<&Instances<K>> {
        self.map.get(alias)
    }

    pub fn get_mut(&mut self, alias: &PartAlias) -> Option<&mut Instances<K>> {
        self.map.get_mut(alias)
    }

    pub fn insert(&mut self, alias: PartAlias, device: &wgpu::Device) {
        self.map
            .insert(alias.clone(), Instances::new(device, alias));
    }
}
