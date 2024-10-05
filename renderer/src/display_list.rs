use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    hash::Hash,
    mem,
    ops::{Range, RangeInclusive},
    sync::Mutex,
};

use cgmath::SquareMatrix;
use ldraw::{
    color::{Color, ColorCatalog, ColorReference},
    Matrix4, PartAlias, Vector4,
};
use ldraw_ir::model::{GroupId, Model, Object, ObjectGroup, ObjectId, ObjectInstance};
use uuid::Uuid;
use wgpu::util::DeviceExt;

use crate::{Entity, GpuUpdate, GpuUpdateResult};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    model_matrix: [[f32; 4]; 4],
    color: [f32; 4],
    edge_color: [f32; 4],
}

impl InstanceData {
    pub fn get_matrix(&self) -> Matrix4 {
        self.model_matrix.into()
    }

    pub fn get_color(&self) -> Vector4 {
        self.color.into()
    }

    pub fn get_edge_color(&self) -> Vector4 {
        self.edge_color.into()
    }
}

#[derive(Debug)]
struct InstanceTransaction<K> {
    rows_to_insert: HashMap<K, (Matrix4, Vector4, Vector4)>,
    rows_to_remove: Vec<K>,
    changed_indices: Vec<usize>,
}

impl<K> Default for InstanceTransaction<K> {
    fn default() -> Self {
        Self {
            rows_to_insert: HashMap::new(),
            rows_to_remove: Vec::new(),
            changed_indices: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Instances<K, G> {
    group: G,
    index: HashMap<K, usize>,
    instance_data: Vec<InstanceData>,

    pub instance_buffer: Option<wgpu::Buffer>,

    transaction: Mutex<Option<InstanceTransaction<K>>>,
}

impl<K, G> Instances<K, G> {
    pub fn count(&self) -> usize {
        self.instance_data.len()
    }

    pub fn range(&self) -> Range<u32> {
        0..self.count() as u32
    }

    fn update_buffer_partial(&self, queue: &wgpu::Queue, range: RangeInclusive<usize>) {
        if let Some(buffer) = &self.instance_buffer {
            queue.write_buffer(
                buffer,
                (range.start() * mem::size_of::<InstanceData>()) as wgpu::BufferAddress,
                bytemuck::cast_slice(&self.instance_data[range]),
            )
        }
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

impl<K, G: Display> Instances<K, G> {
    fn rebuild_buffer(&mut self, device: &wgpu::Device) {
        self.instance_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Instance buffer for {}", self.group)),
                contents: bytemuck::cast_slice(&self.instance_data),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
        );
    }
}

impl<
        K: Clone + Debug + Eq + PartialEq + Hash + Display,
        G: Clone + Eq + PartialEq + Hash + Display,
    > Instances<K, G>
{
    pub fn new(group: G) -> Self {
        Self {
            group,
            index: HashMap::new(),
            instance_data: Vec::new(),

            instance_buffer: None,

            transaction: Mutex::new(None),
        }
    }
}

#[derive(Debug)]
pub enum InstanceOps<K> {
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
    UpdateAlpha {
        key: K,
        alpha: f32,
    },
    Remove(K),
}

impl<K: Clone + Eq + PartialEq + Hash, G: Display> GpuUpdate for Instances<K, G> {
    type Mutator = InstanceOps<K>;

    fn mutate(&mut self, mutator: Self::Mutator) -> GpuUpdateResult<Self::Mutator> {
        let mut tr_lock = self.transaction.lock().unwrap();
        let tr = tr_lock.get_or_insert_with(Default::default);

        match mutator {
            InstanceOps::Insert {
                key,
                matrix,
                color,
                edge_color,
            } => {
                tr.rows_to_insert.insert(key, (matrix, color, edge_color));
            }
            InstanceOps::Remove(key) => {
                tr.rows_to_remove.push(key);
            }
            InstanceOps::Update {
                key,
                matrix,
                color,
                edge_color,
            } => {
                if let Some(entry_idx) = self.index.get(&key).cloned() {
                    let data = &mut self.instance_data[entry_idx];
                    data.model_matrix = matrix.into();
                    data.color = color.into();
                    data.edge_color = edge_color.into();
                    tr.changed_indices.push(entry_idx);
                } else if let Some(entry) = tr.rows_to_insert.get_mut(&key) {
                    entry.0 = matrix;
                    entry.1 = color;
                    entry.2 = edge_color;
                }
            }
            InstanceOps::UpdateMatrix { key, matrix } => {
                if let Some(entry_idx) = self.index.get(&key).cloned() {
                    let data = &mut self.instance_data[entry_idx];
                    data.model_matrix = matrix.into();
                    tr.changed_indices.push(entry_idx);
                } else if let Some(entry) = tr.rows_to_insert.get_mut(&key) {
                    entry.0 = matrix;
                }
            }
            InstanceOps::UpdateColor {
                key,
                color,
                edge_color,
            } => {
                if let Some(entry_idx) = self.index.get(&key).cloned() {
                    let data = &mut self.instance_data[entry_idx];
                    data.color = color.into();
                    data.edge_color = edge_color.into();
                    tr.changed_indices.push(entry_idx);
                } else if let Some(entry) = tr.rows_to_insert.get_mut(&key) {
                    entry.1 = color;
                    entry.2 = edge_color;
                }
            }
            InstanceOps::UpdateAlpha { key, alpha } => {
                if let Some(entry_idx) = self.index.get(&key).cloned() {
                    let data = &mut self.instance_data[entry_idx];
                    data.color[3] = alpha;
                    data.edge_color[3] = alpha;
                    tr.changed_indices.push(entry_idx);
                } else if let Some(entry) = tr.rows_to_insert.get_mut(&key) {
                    entry.1.w = alpha;
                    entry.2.w = alpha;
                }
            }
        }

        GpuUpdateResult::Modified
    }

    fn handle_gpu_update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(mut tr) = self.transaction.lock().unwrap().take() else {
            return;
        };

        let mut layout_changed = false;

        let mut rows_to_remove = tr
            .rows_to_remove
            .into_iter()
            .filter_map(|key| self.index.get(&key).map(|v| (key, *v)))
            .collect::<Vec<_>>();
        rows_to_remove.sort_by_key(|v| std::cmp::Reverse(v.1));

        for (key, (matrix, color, edge_color)) in tr.rows_to_insert.into_iter() {
            if let Some((old_key, idx_to_reuse)) = rows_to_remove.pop() {
                // Take over removed rows and fill with inserted ones if available
                let data = &mut self.instance_data[idx_to_reuse];
                data.model_matrix = matrix.into();
                data.color = color.into();
                data.edge_color = edge_color.into();
                self.index.remove(&old_key);
                self.index.insert(key, idx_to_reuse);
                tr.changed_indices.push(idx_to_reuse);
            } else {
                // Insert new rows
                layout_changed = true;
                self.instance_data.push(InstanceData {
                    model_matrix: matrix.into(),
                    color: color.into(),
                    edge_color: edge_color.into(),
                });
                self.index.insert(key, self.instance_data.len() - 1);
            }
        }

        // Remove rows
        if !rows_to_remove.is_empty() {
            let reverse_lookup = self
                .index
                .clone()
                .into_iter()
                .map(|(k, v)| (v, k))
                .collect::<HashMap<_, _>>();

            for (key, index) in rows_to_remove.iter() {
                let last = self.instance_data.len() - 1;
                if let Some(last_key) = reverse_lookup.get(&last) {
                    self.index.insert(last_key.clone(), *index);
                }
                self.index.remove(key);
                self.instance_data.swap_remove(*index);
                layout_changed = true;
            }
        }

        if layout_changed || self.instance_buffer.is_none() {
            self.rebuild_buffer(device);
        } else if !tr.changed_indices.is_empty() {
            tr.changed_indices.sort();
            let mut start = tr.changed_indices[0];
            let mut end = start;
            for index in tr.changed_indices {
                if index > end + 1 {
                    self.update_buffer_partial(queue, start..=end);
                    start = index;
                    end = index;
                } else {
                    end = index;
                }
            }
            self.update_buffer_partial(queue, start..=end);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum GroupKind {
    Opaque,
    Translucent,
}

impl GroupKind {
    pub fn is_translucent(&self) -> bool {
        matches!(self, GroupKind::Translucent)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct Group<G>(GroupKind, G);

#[derive(Debug, Default)]
pub struct DisplayList<K, G> {
    map: HashMap<Group<G>, Entity<Instances<K, G>>>,
    lookup_table: HashMap<K, Group<G>>,
}

impl<K, G> DisplayList<K, G> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            lookup_table: HashMap::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&G, bool, &Entity<Instances<K, G>>)> {
        self.map
            .iter()
            .map(|(k, v)| (&k.1, matches!(k.0, GroupKind::Translucent), v))
    }

    pub fn iter_opaque(&self) -> impl Iterator<Item = (&G, &Entity<Instances<K, G>>)> {
        self.map.iter().filter_map(|(k, v)| {
            if matches!(k.0, GroupKind::Opaque) {
                Some((&k.1, v))
            } else {
                None
            }
        })
    }

    pub fn iter_translucent(&self) -> impl Iterator<Item = (&G, &Entity<Instances<K, G>>)> {
        self.map.iter().filter_map(|(k, v)| {
            if matches!(k.0, GroupKind::Translucent) {
                Some((&k.1, v))
            } else {
                None
            }
        })
    }
}

impl<
        K: Clone + Debug + Eq + PartialEq + Hash + Display,
        G: Clone + Eq + PartialEq + Hash + Display,
    > DisplayList<K, G>
{
    fn get_or_create(&mut self, group: Group<G>) -> &mut Entity<Instances<K, G>> {
        self.map
            .entry(group.clone())
            .or_insert_with(|| Instances::new(group.1).into())
    }

    pub fn get_by_key(&self, k: &K) -> Option<&Entity<Instances<K, G>>> {
        if let Some(group) = self.lookup_table.get(k) {
            self.map.get(group)
        } else {
            None
        }
    }
}

fn uuid_xor(a: ObjectId, b: ObjectId) -> ObjectId {
    let ba = Uuid::from(a).to_bytes_le();
    let bb = Uuid::from(b).to_bytes_le();

    let bc: Vec<_> = ba.iter().zip(bb).map(|(x, y)| x ^ y).collect();
    Uuid::from_slice(&bc).unwrap().into()
}

impl<P: Clone + Eq + PartialEq + Hash + From<PartAlias> + Display> DisplayList<ObjectId, P> {
    fn expand_object_group(
        ops: &mut Vec<DisplayListOps<ObjectId, P>>,
        color_catalog: &ColorCatalog,
        parent_id: ObjectId,
        groups: &HashMap<GroupId, ObjectGroup<P>>,
        objects: &[Object<P>],
        matrix: Matrix4,
        color: ColorReference,
    ) {
        for object in objects.iter() {
            match &object.data {
                ObjectInstance::Part(p) => {
                    let local_matrix = matrix * p.matrix;
                    let color_ref = if p.color.is_current() {
                        &color
                    } else {
                        &p.color
                    };
                    let color = match color_ref {
                        ColorReference::Color(c) => c,
                        _ => color_catalog.get(&0).unwrap(),
                    };
                    ops.push(DisplayListOps::Insert {
                        group: p.part.clone(),
                        key: uuid_xor(parent_id, object.id),
                        matrix: local_matrix,
                        color: color.clone(),
                        alpha: None,
                    });
                }
                ObjectInstance::PartGroup(g) => {
                    if let Some(group) = groups.get(&g.group_id) {
                        let color = if g.color.is_current() {
                            &color
                        } else {
                            &g.color
                        }
                        .clone();

                        Self::expand_object_group(
                            ops,
                            color_catalog,
                            uuid_xor(parent_id, object.id),
                            groups,
                            &group.objects,
                            matrix * g.matrix,
                            color,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub fn from_model(
        model: &Model<P>,
        group_id: Option<GroupId>,
        color_catalog: &ColorCatalog,
    ) -> Entity<Self> {
        let mut display_list = Entity::new(Self::new());

        let objects = match group_id {
            Some(group_id) => model.object_groups.get(&group_id).map(|v| &v.objects),
            None => Some(&model.objects),
        };

        let mut ops = vec![];
        if let Some(objects) = objects {
            Self::expand_object_group(
                &mut ops,
                color_catalog,
                Uuid::nil().into(),
                &model.object_groups,
                objects,
                Matrix4::identity(),
                ColorReference::Color(color_catalog.get(&0).cloned().unwrap()),
            )
        }
        display_list.mutate_all(ops.into_iter());

        display_list
    }
}

pub struct DisplayListOpsReinstantiate<G, K> {
    group: Group<G>,
    key: K,
    matrix: Matrix4,
    color: Vector4,
    edge_color: Vector4,
}

pub enum DisplayListOps<K, G> {
    Insert {
        group: G,
        key: K,
        matrix: Matrix4,
        color: Color,
        alpha: Option<f32>,
    },
    Update {
        key: K,
        matrix: Matrix4,
        color: Color,
    },
    UpdateMatrix {
        key: K,
        matrix: Matrix4,
    },
    UpdateColor {
        key: K,
        color: Color,
    },
    UpdateAlpha {
        key: K,
        alpha: f32,
    },
    Remove {
        key: K,
    },
    _Reinstantiate(DisplayListOpsReinstantiate<G, K>),
}

impl<
        K: Clone + Debug + Eq + PartialEq + Hash + Display,
        G: Clone + Eq + PartialEq + Hash + Display,
    > GpuUpdate for DisplayList<K, G>
{
    type Mutator = DisplayListOps<K, G>;

    fn mutate(&mut self, mutator: Self::Mutator) -> GpuUpdateResult<Self::Mutator> {
        match mutator {
            DisplayListOps::Insert {
                group,
                key,
                matrix,
                color,
                alpha,
            } => {
                let mut main_color: Vector4 = color.color.into();
                let mut edge_color: Vector4 = color.edge.into();

                if self.lookup_table.contains_key(&key) {
                    GpuUpdateResult::NotModified
                } else {
                    let is_translucent = main_color.w < 1.0 || alpha.unwrap_or(1.0) < 1.0;

                    let group = if is_translucent {
                        Group(GroupKind::Translucent, group)
                    } else {
                        Group(GroupKind::Opaque, group)
                    };

                    self.lookup_table.insert(key.clone(), group.clone());

                    if let Some(alpha) = alpha {
                        main_color.w = alpha;
                        edge_color.w = alpha;
                    }

                    self.lookup_table.insert(key.clone(), group.clone());

                    let entity = self.get_or_create(group);
                    entity
                        .mutate(InstanceOps::Insert {
                            key,
                            matrix,
                            color: main_color,
                            edge_color,
                        })
                        .into()
                }
            }
            DisplayListOps::Update { key, matrix, color } => {
                if let Some(group) = self.lookup_table.get(&key) {
                    if group.0.is_translucent() != color.is_translucent() {
                        let id = group.1.clone();
                        let new_group = if color.is_translucent() {
                            Group(GroupKind::Translucent, id)
                        } else {
                            Group(GroupKind::Opaque, id)
                        };

                        GpuUpdateResult::AdditionalMutations {
                            modified: false,
                            mutations: vec![DisplayListOps::_Reinstantiate(
                                DisplayListOpsReinstantiate {
                                    group: new_group,
                                    key,
                                    matrix,
                                    color: color.color.into(),
                                    edge_color: color.edge.into(),
                                },
                            )],
                        }
                    } else if let Some(instances) = self.map.get_mut(group) {
                        instances
                            .mutate(InstanceOps::Update {
                                key,
                                matrix,
                                color: color.color.into(),
                                edge_color: color.edge.into(),
                            })
                            .into()
                    } else {
                        GpuUpdateResult::NotModified
                    }
                } else {
                    GpuUpdateResult::NotModified
                }
            }
            DisplayListOps::UpdateAlpha { key, alpha } => {
                let is_translucent = alpha < 1.0;
                if let Some((group, instances)) = self
                    .lookup_table
                    .get(&key)
                    .and_then(|g| self.map.get_mut(g).map(|v| (g.clone(), v)))
                {
                    if group.0.is_translucent() != is_translucent {
                        let id = group.1.clone();
                        let group = if is_translucent {
                            Group(GroupKind::Translucent, id)
                        } else {
                            Group(GroupKind::Opaque, id)
                        };

                        let Some(index) = instances.get().index.get(&key) else {
                            return GpuUpdateResult::NotModified;
                        };

                        let instance = instances.get().instance_data[*index];

                        let mut color = instance.get_color();
                        color.w = alpha;
                        let mut edge_color = instance.get_edge_color();
                        edge_color.w = alpha;

                        GpuUpdateResult::AdditionalMutations {
                            modified: false,
                            mutations: vec![DisplayListOps::_Reinstantiate(
                                DisplayListOpsReinstantiate {
                                    group,
                                    key,
                                    matrix: instance.get_matrix(),
                                    color,
                                    edge_color,
                                },
                            )],
                        }
                    } else if let Some(entity) = self.map.get_mut(&group) {
                        entity
                            .mutate(InstanceOps::UpdateAlpha { key, alpha })
                            .into()
                    } else {
                        GpuUpdateResult::NotModified
                    }
                } else {
                    GpuUpdateResult::NotModified
                }
            }
            DisplayListOps::UpdateColor { key, color } => {
                if let Some((group, instances)) = self
                    .lookup_table
                    .get(&key)
                    .and_then(|g| self.map.get_mut(g).map(|v| (g.clone(), v)))
                {
                    if group.0.is_translucent() != color.is_translucent() {
                        let id = group.1.clone();
                        let group = if color.is_translucent() {
                            Group(GroupKind::Translucent, id)
                        } else {
                            Group(GroupKind::Opaque, id)
                        };

                        let Some(index) = instances.get().index.get(&key) else {
                            return GpuUpdateResult::NotModified;
                        };

                        let instance = instances.get().instance_data[*index];

                        GpuUpdateResult::AdditionalMutations {
                            modified: false,
                            mutations: vec![DisplayListOps::_Reinstantiate(
                                DisplayListOpsReinstantiate {
                                    group,
                                    key,
                                    matrix: instance.get_matrix(),
                                    color: color.color.into(),
                                    edge_color: color.edge.into(),
                                },
                            )],
                        }
                    } else if let Some(entity) = self.map.get_mut(&group) {
                        entity
                            .mutate(InstanceOps::UpdateColor {
                                key,
                                color: color.color.into(),
                                edge_color: color.edge.into(),
                            })
                            .into()
                    } else {
                        GpuUpdateResult::NotModified
                    }
                } else {
                    GpuUpdateResult::NotModified
                }
            }
            DisplayListOps::UpdateMatrix { key, matrix } => {
                let Some(group) = self.lookup_table.get(&key) else {
                    return GpuUpdateResult::NotModified;
                };

                if let Some(entity) = self.map.get_mut(group) {
                    entity
                        .mutate(InstanceOps::UpdateMatrix { key, matrix })
                        .into()
                } else {
                    GpuUpdateResult::NotModified
                }
            }
            DisplayListOps::Remove { key } => {
                let Some(group) = self.lookup_table.remove(&key) else {
                    return GpuUpdateResult::NotModified;
                };
                let Some(entity) = self.map.get_mut(&group) else {
                    return GpuUpdateResult::NotModified;
                };

                if entity.mutate(InstanceOps::Remove(key.clone())) {
                    self.lookup_table.remove(&key);
                    if (*entity).count() == 0 {
                        self.map.remove(&group);
                    }

                    GpuUpdateResult::Modified
                } else {
                    GpuUpdateResult::NotModified
                }
            }
            DisplayListOps::_Reinstantiate(DisplayListOpsReinstantiate {
                group,
                key,
                matrix,
                color,
                edge_color,
            }) => {
                let Some(prev_group) = self.lookup_table.remove(&key) else {
                    return GpuUpdateResult::NotModified;
                };

                let Some(entity) = self.map.get_mut(&prev_group) else {
                    return GpuUpdateResult::NotModified;
                };

                entity.mutate(InstanceOps::Remove(key.clone()));

                if self
                    .get_or_create(group.clone())
                    .mutate(InstanceOps::Insert {
                        key: key.clone(),
                        matrix,
                        color,
                        edge_color,
                    })
                {
                    self.lookup_table.insert(key.clone(), group);

                    GpuUpdateResult::Modified
                } else {
                    GpuUpdateResult::NotModified
                }
            }
        }
    }

    fn handle_gpu_update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        for entity in self.map.values_mut() {
            entity.update(device, queue);
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SelectionInstanceData {
    model_matrix: [[f32; 4]; 4],
    instance_id: u32,
    _padding: [u32; 3],
}

#[derive(Debug)]
pub struct SelectionInstances {
    instance_data: Vec<SelectionInstanceData>,

    pub instance_buffer: wgpu::Buffer,
}

impl SelectionInstances {
    pub fn new(device: &wgpu::Device, instance_data: Vec<SelectionInstanceData>) -> Self {
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance buffer for object selections"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            instance_data,
            instance_buffer,
        }
    }

    pub fn count(&self) -> usize {
        self.instance_data.len()
    }

    pub fn range(&self) -> Range<u32> {
        0..self.count() as u32
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<SelectionInstanceData>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

#[derive(Debug, Default)]
pub struct SelectionDisplayList<G, K> {
    map: HashMap<G, SelectionInstances>,
    lookup_table: HashMap<u32, K>,
}

impl<G, K: Clone> SelectionDisplayList<G, K> {
    pub fn new(map: HashMap<G, SelectionInstances>, lookup_table: HashMap<u32, K>) -> Self {
        Self { map, lookup_table }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&G, &SelectionInstances)> {
        self.map.iter()
    }

    pub fn get_matches(
        &self,
        result: impl Iterator<Item = u32> + 'static,
    ) -> impl Iterator<Item = K> + '_ {
        result.filter_map(|v| self.lookup_table.get(&v).cloned())
    }
}

impl<G: Clone + Eq + PartialEq + Hash + From<PartAlias> + Display>
    SelectionDisplayList<G, ObjectId>
{
    #[allow(clippy::too_many_arguments)]
    fn expand_object_group(
        data: &mut HashMap<G, Vec<SelectionInstanceData>>,
        lookup_table: &mut HashMap<u32, ObjectId>,
        cur_instance_id: &mut u32,
        use_parent_object_id: bool,
        parent_id: ObjectId,
        groups: &HashMap<GroupId, ObjectGroup<G>>,
        objects: &[Object<G>],
        matrix: Matrix4,
        depth: u32,
    ) {
        for object in objects.iter() {
            match &object.data {
                ObjectInstance::Part(p) => {
                    let id = if depth == 0 {
                        object.id
                    } else if use_parent_object_id {
                        parent_id
                    } else {
                        uuid_xor(parent_id, object.id)
                    };
                    lookup_table.insert(*cur_instance_id, id);
                    data.entry(p.part.clone())
                        .or_default()
                        .push(SelectionInstanceData {
                            model_matrix: (matrix * p.matrix).into(),
                            instance_id: *cur_instance_id,
                            _padding: [0; 3],
                        });
                    *cur_instance_id += 1;
                }
                ObjectInstance::PartGroup(g) => {
                    if let Some(group) = groups.get(&g.group_id) {
                        Self::expand_object_group(
                            data,
                            lookup_table,
                            cur_instance_id,
                            use_parent_object_id,
                            object.id,
                            groups,
                            &group.objects,
                            matrix * g.matrix,
                            depth + 1,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub fn from_model(
        model: &Model<G>,
        group_id: Option<GroupId>,
        device: &wgpu::Device,
        instance_id_base: u32,
        flatten_group: bool,
    ) -> Self {
        let objects = match group_id {
            Some(group_id) => model.object_groups.get(&group_id).map(|v| &v.objects),
            None => Some(&model.objects),
        };

        let mut instance_data = HashMap::new();
        let mut lookup_table = HashMap::new();
        let mut instance_id = instance_id_base;

        if let Some(objects) = objects {
            Self::expand_object_group(
                &mut instance_data,
                &mut lookup_table,
                &mut instance_id,
                flatten_group,
                Uuid::nil().into(),
                &model.object_groups,
                objects,
                Matrix4::identity(),
                0,
            );
        }

        let map = instance_data
            .into_iter()
            .map(|(k, v)| (k, SelectionInstances::new(device, v)))
            .collect();

        Self::new(map, lookup_table)
    }
}
