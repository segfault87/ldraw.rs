use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Display},
    hash::Hash,
    mem,
    ops::{Range, RangeInclusive},
};

use cgmath::SquareMatrix;
use ldraw::{
    color::{Color, ColorCatalog, ColorReference},
    Matrix4, PartAlias, Vector4,
};
use ldraw_ir::model::{GroupId, Model, Object, ObjectGroup, ObjectId, ObjectInstance};
use uuid::Uuid;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    model_matrix: [[f32; 4]; 4],
    color: [f32; 4],
    edge_color: [f32; 4],
}

#[derive(Debug)]
pub struct Instances<K, G> {
    group: G,
    index: HashMap<K, usize>,
    instance_data: Vec<InstanceData>,

    pub instance_buffer: wgpu::Buffer,
}

impl<K, G> Instances<K, G> {
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
            bytemuck::cast_slice(&self.instance_data[range]),
        )
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
        self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Instance buffer for {}", self.group)),
            contents: bytemuck::cast_slice(&self.instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
    }
}

impl<
        K: Clone + Debug + Eq + PartialEq + Hash + Display,
        G: Clone + Eq + PartialEq + Hash + Display,
    > Instances<K, G>
{
    pub fn new(device: &wgpu::Device, group: G) -> Self {
        let instance_data = Vec::new();
        let index = HashMap::new();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Instance buffer for {}", group)),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            group,
            index,
            instance_data,

            instance_buffer,
        }
    }

    pub fn modify<F: FnOnce(&mut InstanceTransaction<K, G>) -> bool>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        f: F,
    ) {
        let mut transaction = InstanceTransaction::new(self);

        if f(&mut transaction) {
            transaction.commit(device, queue);
        }
    }
}

#[derive(Debug)]
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
    UpdateAlpha {
        key: K,
        alpha: f32,
    },
    Remove(K),
}

pub struct InstanceTransaction<'a, K, G> {
    instances: &'a mut Instances<K, G>,
    ops: Vec<Ops<K>>,
}

impl<
        'a,
        K: Clone + Debug + Eq + PartialEq + Hash + Display,
        G: Clone + Eq + PartialEq + Hash + Display,
    > InstanceTransaction<'a, K, G>
{
    pub fn new(instances: &'a mut Instances<K, G>) -> Self {
        Self {
            instances,
            ops: Vec::new(),
        }
    }

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

    pub fn update_alpha(&mut self, key: K, alpha: f32) {
        self.ops.push(Ops::UpdateAlpha { key, alpha });
    }

    pub fn remove(&mut self, key: K) {
        self.ops.push(Ops::Remove(key));
    }

    fn push_ops(&mut self, ops: Ops<K>) {
        self.ops.push(ops);
    }

    fn commit(mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
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
                        entry.0 = matrix;
                        entry.1 = color;
                        entry.2 = edge_color;
                    }
                }
                Ops::UpdateMatrix { key, matrix } => {
                    if let Some(entry_idx) = instances.index.get(&key).cloned() {
                        let data = &mut instances.instance_data[entry_idx];
                        data.model_matrix = matrix.into();
                        changed_indices.push(entry_idx);
                    } else if let Some(entry) = rows_to_insert.get_mut(&key) {
                        entry.0 = matrix;
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
                        entry.1 = color;
                        entry.2 = edge_color;
                    }
                }
                Ops::UpdateAlpha { key, alpha } => {
                    if let Some(entry_idx) = instances.index.get(&key).cloned() {
                        let data = &mut instances.instance_data[entry_idx];
                        data.color[3] = alpha;
                        data.edge_color[3] = alpha;
                        changed_indices.push(entry_idx);
                    } else if let Some(entry) = rows_to_insert.get_mut(&key) {
                        entry.1.w = alpha;
                        entry.2.w = alpha;
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
            let mut removed_rows_set = HashSet::new();

            let mut removed = 0;
            for (key, index) in rows_to_remove.iter() {
                instances.instance_data.remove(index - removed);
                removed_rows_set.insert(key);
                removed += 1;
                layout_changed = true;
            }

            // Squash the index
            removed = 0;
            let reverse_lookup = instances
                .index
                .clone()
                .into_iter()
                .map(|(k, v)| (v, k))
                .collect::<HashMap<_, _>>();

            for i in 0..len {
                let v = reverse_lookup.get(&i).expect("Corrupted instance buffer");

                if removed_rows_set.contains(v) {
                    removed += 1;
                    instances.index.remove(v);
                } else if let Some(pos) = instances.index.get_mut(v) {
                    *pos -= removed;
                }
            }
        }

        if layout_changed {
            instances.rebuild_buffer(device);
        } else if !changed_indices.is_empty() {
            changed_indices.sort();
            let mut start = changed_indices[0];
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
    map: HashMap<Group<G>, Instances<K, G>>,
    lookup_table: HashMap<K, Group<G>>,
}

impl<K, G> DisplayList<K, G> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            lookup_table: HashMap::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&G, bool, &Instances<K, G>)> {
        self.map
            .iter()
            .map(|(k, v)| (&k.1, matches!(k.0, GroupKind::Translucent), v))
    }

    pub fn iter_opaque(&self) -> impl Iterator<Item = (&G, &Instances<K, G>)> {
        self.map.iter().filter_map(|(k, v)| {
            if matches!(k.0, GroupKind::Opaque) {
                Some((&k.1, v))
            } else {
                None
            }
        })
    }

    pub fn iter_translucent(&self) -> impl Iterator<Item = (&G, &Instances<K, G>)> {
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
    fn get_or_create(&mut self, group: Group<G>, device: &wgpu::Device) -> &mut Instances<K, G> {
        self.map
            .entry(group.clone())
            .or_insert_with(|| Instances::new(device, group.1))
    }

    pub fn get_by_key(&self, k: &K) -> Option<&Instances<K, G>> {
        if let Some(group) = self.lookup_table.get(k) {
            self.map.get(group)
        } else {
            None
        }
    }

    pub fn modify<F: FnOnce(&mut DisplayListTransaction<K, G>) -> bool>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        f: F,
    ) {
        let mut transaction = DisplayListTransaction::new(self);

        if f(&mut transaction) {
            transaction.commit(device, queue);
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
        t: &mut DisplayListTransaction<ObjectId, P>,
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
                    t.insert(
                        p.part.clone(),
                        uuid_xor(parent_id, object.id),
                        local_matrix,
                        color,
                        None,
                    );
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
                            t,
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
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_catalog: &ColorCatalog,
    ) -> Self {
        let mut display_list = Self::new();

        let objects = match group_id {
            Some(group_id) => model.object_groups.get(&group_id).map(|v| &v.objects),
            None => Some(&model.objects),
        };

        if let Some(objects) = objects {
            display_list.modify(device, queue, |t| {
                Self::expand_object_group(
                    t,
                    color_catalog,
                    Uuid::nil().into(),
                    &model.object_groups,
                    objects,
                    Matrix4::identity(),
                    ColorReference::Color(color_catalog.get(&0).cloned().unwrap()),
                );
                true
            });
        }

        display_list
    }
}

pub struct DisplayListTransaction<'a, K, G> {
    display_list: &'a mut DisplayList<K, G>,
    lookup_table: HashMap<K, Group<G>>,
    ops: HashMap<Group<G>, Vec<Ops<K>>>,
}

impl<
        'a,
        K: Clone + Debug + Eq + PartialEq + Hash + Display,
        G: Clone + Eq + PartialEq + Hash + Display,
    > DisplayListTransaction<'a, K, G>
{
    fn new(display_list: &'a mut DisplayList<K, G>) -> Self {
        let lookup_table = display_list.lookup_table.clone();

        Self {
            display_list,
            lookup_table,
            ops: HashMap::new(),
        }
    }

    pub fn insert(&mut self, group: G, key: K, matrix: Matrix4, color: &Color, alpha: Option<f32>) {
        self.do_insert(
            group,
            key,
            matrix,
            color.color.into(),
            color.edge.into(),
            alpha,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn do_insert(
        &mut self,
        group: G,
        key: K,
        matrix: Matrix4,
        color: Vector4,
        edge_color: Vector4,
        alpha: Option<f32>,
    ) {
        if self.lookup_table.contains_key(&key) {
            return;
        }

        let is_translucent = color.w < 1.0 || alpha.unwrap_or(1.0) < 1.0;

        let group = if is_translucent {
            Group(GroupKind::Translucent, group)
        } else {
            Group(GroupKind::Opaque, group)
        };

        self.lookup_table.insert(key.clone(), group.clone());

        let mut color_vec: Vector4 = color;
        let mut edge_color_vec: Vector4 = edge_color;

        if alpha.unwrap_or(1.0) < 1.0 {
            let alpha = alpha.unwrap();
            color_vec.w = alpha;
            edge_color_vec.w = alpha;
        }

        self.ops.entry(group).or_default().push(Ops::Insert {
            key,
            matrix,
            color: color_vec,
            edge_color: edge_color_vec,
        });
    }

    pub fn update(&mut self, key: K, matrix: Matrix4, color: &Color) {
        if let Some(group) = self.lookup_table.get(&key) {
            if group.0.is_translucent() != color.is_translucent() {
                let id = group.1.clone();
                self.remove(key.clone());
                self.insert(id, key, matrix, color, None);
            } else {
                self.ops
                    .entry(group.clone())
                    .or_default()
                    .push(Ops::Update {
                        key,
                        matrix,
                        color: color.color.into(),
                        edge_color: color.edge.into(),
                    });
            }
        }
    }

    pub fn update_matrix(&mut self, key: K, matrix: Matrix4) {
        if let Some(group) = self.lookup_table.get(&key) {
            self.ops
                .entry(group.clone())
                .or_default()
                .push(Ops::UpdateMatrix { key, matrix });
        }
    }

    pub fn update_color(&mut self, key: K, color: &Color) {
        if let Some(group) = self.lookup_table.get(&key) {
            if group.0.is_translucent() != color.is_translucent() {
                let matrix = {
                    // Take matrix from previous entries (quite cumbersome)
                    let mut latest = None;
                    if let Some(ops) = self.ops.get(group) {
                        for op in ops.iter() {
                            match op {
                                Ops::Insert {
                                    key: okey, matrix, ..
                                } => {
                                    if okey == &key {
                                        latest = Some(*matrix);
                                    }
                                }
                                Ops::Update {
                                    key: okey, matrix, ..
                                } => {
                                    if okey == &key {
                                        latest = Some(*matrix);
                                    }
                                }
                                Ops::UpdateMatrix { key: okey, matrix } => {
                                    if okey == &key {
                                        latest = Some(*matrix);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    if latest.is_none() {
                        if let Some(instances) = self.display_list.get_by_key(&key) {
                            if let Some(index) = instances.index.get(&key) {
                                latest = Some(instances.instance_data[*index].model_matrix.into());
                            }
                        }
                    }
                    match latest {
                        Some(v) => v,
                        None => {
                            panic!("Corrupted transaction.")
                        }
                    }
                };
                let id = group.1.clone();
                self.remove(key.clone());
                self.insert(id, key, matrix, color, None);
            } else {
                self.ops
                    .entry(group.clone())
                    .or_default()
                    .push(Ops::UpdateColor {
                        key,
                        color: color.color.into(),
                        edge_color: color.edge.into(),
                    });
            }
        }
    }

    pub fn update_alpha(&mut self, key: K, alpha: f32) {
        let is_translucent = alpha < 1.0;
        if let Some(group) = self.lookup_table.get(&key) {
            if group.0.is_translucent() != is_translucent {
                let entry = {
                    // Take matrix from previous entries (quite cumbersome)
                    let mut latest = None;
                    if let Some(ops) = self.ops.get(group) {
                        for op in ops.iter() {
                            match op {
                                Ops::Insert {
                                    key: okey,
                                    matrix,
                                    color,
                                    edge_color,
                                } => {
                                    if okey == &key {
                                        latest = Some((*matrix, *color, *edge_color));
                                    }
                                }
                                Ops::Update {
                                    key: okey,
                                    matrix,
                                    color,
                                    edge_color,
                                } => {
                                    if okey == &key {
                                        latest = Some((*matrix, *color, *edge_color));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    if latest.is_none() {
                        if let Some(instances) = self.display_list.get_by_key(&key) {
                            if let Some(index) = instances.index.get(&key) {
                                latest = Some((
                                    instances.instance_data[*index].model_matrix.into(),
                                    instances.instance_data[*index].color.into(),
                                    instances.instance_data[*index].edge_color.into(),
                                ));
                            }
                        }
                    }
                    match latest {
                        Some(v) => v,
                        None => {
                            panic!("Corrupted transaction.")
                        }
                    }
                };
                let id = group.1.clone();
                let mut color = entry.1;
                color.w = alpha;
                let mut edge_color = entry.2;
                edge_color.w = alpha;
                self.remove(key.clone());
                self.do_insert(id, key, entry.0, color, edge_color, Some(alpha));
            } else {
                self.ops
                    .entry(group.clone())
                    .or_default()
                    .push(Ops::UpdateAlpha { key, alpha });
            }
        }
    }

    pub fn remove(&mut self, key: K) {
        if let Some(group) = self.lookup_table.remove(&key) {
            self.ops.entry(group).or_default().push(Ops::Remove(key));
        }
    }

    fn commit(self, device: &wgpu::Device, queue: &wgpu::Queue) {
        for (part, ops) in self.ops.into_iter() {
            let instances = self.display_list.get_or_create(part, device);
            instances.modify(device, queue, |t| {
                for op in ops {
                    t.push_ops(op);
                }

                true
            });
        }

        self.display_list.map.retain(|_k, v| v.count() > 0);
        self.display_list.lookup_table = self.lookup_table;
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
