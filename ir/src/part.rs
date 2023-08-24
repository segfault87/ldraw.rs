use std::{
    cell::RefCell, collections::HashMap, f32, fmt::Debug, ops::Deref, rc::Rc, sync::Arc, vec::Vec,
};

use cgmath::{AbsDiffEq, InnerSpace, Rad, SquareMatrix};
use kdtree::{distance::squared_euclidean, KdTree};
use ldraw::{
    color::{ColorCatalog, ColorReference},
    document::{Document, MultipartDocument},
    elements::{BfcStatement, Command, Meta},
    library::ResolutionResult,
    Matrix4, Vector3, Winding,
};
use serde::{Deserialize, Serialize};

use crate::{geometry::BoundingBox3, MeshGroupKey};

const NORMAL_BLEND_THRESHOLD: Rad<f32> = Rad(f32::consts::FRAC_PI_6);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VertexBuffer(pub Vec<f32>);

impl VertexBuffer {
    pub fn check_range(&self, range: &std::ops::Range<usize>) -> bool {
        self.0.len() >= range.end
    }
}

pub struct VertexBufferBuilder {
    vertices: Vec<Vector3>,
    index_table: KdTree<f32, u32, [f32; 3]>,
    current_index: u32,
}

impl Default for VertexBufferBuilder {
    fn default() -> Self {
        Self {
            vertices: Default::default(),
            index_table: KdTree::new(3),
            current_index: 0,
        }
    }
}

impl VertexBufferBuilder {
    pub fn add(&mut self, vertex: Vector3) -> u32 {
        let vertex_ref: &[f32; 3] = vertex.as_ref();
        if let Ok(entries) = self.index_table.nearest(vertex_ref, 1, &squared_euclidean) {
            if let Some((dist, index)) = entries.first() {
                if dist < &f32::default_epsilon() {
                    return **index;
                }
            }
        }

        let index = self.current_index;
        match self.index_table.add(*vertex_ref, index) {
            Ok(_) => {
                self.vertices.push(vertex);
                self.current_index += 1;
                index
            }
            Err(e) => {
                panic!("Error adding vertex to vertex buffer: {vertex_ref:?} {e}");
            }
        }
    }

    pub fn add_all(&mut self, vertices: impl Iterator<Item = Vector3>) -> Vec<u32> {
        let mut result = vec![];
        for vertex in vertices {
            result.push(self.add(vertex));
        }
        result
    }

    pub fn build(self) -> VertexBuffer {
        let mut result = Vec::new();
        for vertex in self.vertices.iter() {
            let vertex: &[f32; 3] = vertex.as_ref();
            result.extend(vertex);
        }
        VertexBuffer(result)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MeshBuffer {
    pub vertex_indices: Vec<u32>,
    pub normal_indices: Vec<u32>,
}

impl MeshBuffer {
    pub fn len(&self) -> usize {
        self.vertex_indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertex_indices.is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.vertex_indices.len() == self.normal_indices.len()
    }

    pub fn add(
        &mut self,
        vertex_buffer: &mut VertexBufferBuilder,
        vertex: Vector3,
        normal: Vector3,
    ) {
        self.vertex_indices.push(vertex_buffer.add(vertex));
        self.normal_indices.push(vertex_buffer.add(normal));
    }

    pub fn add_indices(&mut self, vertex_indices: Vec<u32>, normal_indices: Vec<u32>) {
        self.vertex_indices.extend(vertex_indices);
        self.normal_indices.extend(normal_indices);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EdgeBuffer {
    pub vertex_indices: Vec<u32>,
    pub colors: Vec<u32>,
}

impl EdgeBuffer {
    pub fn add(
        &mut self,
        vertex_buffer: &mut VertexBufferBuilder,
        vec: Vector3,
        color: &ColorReference,
        top: &ColorReference,
    ) {
        self.vertex_indices.push(vertex_buffer.add(vec));

        let code = if color.is_current() {
            if let Some(c) = top.get_color() {
                2 << 31 | c.code
            } else {
                2 << 30
            }
        } else if color.is_complement() {
            if let Some(c) = top.get_color() {
                c.code
            } else {
                2 << 29
            }
        } else if let Some(c) = color.get_color() {
            2 << 31 | c.code
        } else {
            0
        };
        self.colors.push(code);
    }

    pub fn len(&self) -> usize {
        self.vertex_indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertex_indices.is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.vertex_indices.len() == self.colors.len()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OptionalEdgeBuffer {
    pub vertex_indices: Vec<u32>,
    pub control_1_indices: Vec<u32>,
    pub control_2_indices: Vec<u32>,
    pub direction_indices: Vec<u32>,
    pub colors: Vec<u32>,
}

impl OptionalEdgeBuffer {
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        vertex_buffer: &mut VertexBufferBuilder,
        v1: Vector3,
        v2: Vector3,
        c1: Vector3,
        c2: Vector3,
        color: &ColorReference,
        top: &ColorReference,
    ) {
        let d = v2 - v1;

        self.vertex_indices.push(vertex_buffer.add(v1));
        self.vertex_indices.push(vertex_buffer.add(v2));
        self.control_1_indices.push(vertex_buffer.add(c1));
        self.control_1_indices.push(vertex_buffer.add(c1));
        self.control_2_indices.push(vertex_buffer.add(c2));
        self.control_2_indices.push(vertex_buffer.add(c2));
        self.direction_indices.push(vertex_buffer.add(d));
        self.direction_indices.push(vertex_buffer.add(d));

        let code = if color.is_current() {
            if let Some(c) = top.get_color() {
                2 << 31 | c.code
            } else {
                2 << 30
            }
        } else if color.is_complement() {
            if let Some(c) = top.get_color() {
                c.code
            } else {
                2 << 29
            }
        } else if let Some(c) = color.get_color() {
            2 << 31 | c.code
        } else {
            0
        };
        self.colors.push(code);
        self.colors.push(code);
    }

    pub fn len(&self) -> usize {
        self.vertex_indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertex_indices.is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.vertex_indices.len() == self.control_1_indices.len()
            && self.vertex_indices.len() == self.control_2_indices.len()
            && self.vertex_indices.len() == self.direction_indices.len()
            && self.vertex_indices.len() == self.colors.len()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PartBufferBundle {
    pub vertex_buffer: VertexBuffer,
    pub uncolored_mesh: MeshBuffer,
    pub uncolored_without_bfc_mesh: MeshBuffer,
    pub colored_meshes: HashMap<MeshGroupKey, MeshBuffer>,
    pub edges: EdgeBuffer,
    pub optional_edges: OptionalEdgeBuffer,
}

#[derive(Default)]
pub struct PartBufferBundleBuilder {
    pub vertex_buffer_builder: VertexBufferBuilder,
    uncolored_mesh: MeshBuffer,
    uncolored_without_bfc_mesh: MeshBuffer,
    colored_meshes: HashMap<MeshGroupKey, MeshBuffer>,
    edges: EdgeBuffer,
    optional_edges: OptionalEdgeBuffer,
}

impl PartBufferBundleBuilder {
    pub fn resolve_colors(&mut self, colors: &ColorCatalog) {
        let keys = self.colored_meshes.keys().cloned().collect::<Vec<_>>();
        for key in keys.iter() {
            let val = match self.colored_meshes.remove(key) {
                Some(v) => v,
                None => continue,
            };
            let mut key = key.clone();
            key.resolve_color(colors);
            self.colored_meshes.insert(key, val);
        }
    }

    fn query_mesh<'a>(&'a mut self, group: &MeshGroupKey) -> Option<&'a mut MeshBuffer> {
        match (&group.color_ref, group.bfc) {
            (ColorReference::Current | ColorReference::Complement, true) => {
                Some(&mut self.uncolored_mesh)
            }
            (ColorReference::Current | ColorReference::Complement, false) => {
                Some(&mut self.uncolored_without_bfc_mesh)
            }
            (ColorReference::Color(_), _) => Some(
                self.colored_meshes
                    .entry(group.clone())
                    .or_insert_with(MeshBuffer::default),
            ),
            _ => None,
        }
    }

    pub fn build(self) -> PartBufferBundle {
        PartBufferBundle {
            vertex_buffer: self.vertex_buffer_builder.build(),
            uncolored_mesh: self.uncolored_mesh,
            uncolored_without_bfc_mesh: self.uncolored_without_bfc_mesh,
            colored_meshes: self.colored_meshes,
            edges: self.edges,
            optional_edges: self.optional_edges,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartMetadata {
    pub name: String,
    pub description: String,
    pub author: String,
    pub extras: HashMap<String, String>,
}

impl From<&Document> for PartMetadata {
    fn from(document: &Document) -> PartMetadata {
        PartMetadata {
            name: document.name.clone(),
            description: document.description.clone(),
            author: document.author.clone(),
            extras: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Part {
    pub metadata: PartMetadata,
    pub geometry: PartBufferBundle,
    pub bounding_box: BoundingBox3,
    pub rotation_center: Vector3,
}

impl Part {
    pub fn new(
        metadata: PartMetadata,
        geometry: PartBufferBundle,
        bounding_box: BoundingBox3,
        rotation_center: Vector3,
    ) -> Self {
        Part {
            metadata,
            geometry,
            bounding_box,
            rotation_center,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FaceVertex {
    position: Vector3,
    normal: Vector3,
}

#[derive(Clone, Debug, PartialEq)]
enum FaceVertices {
    Triangle([FaceVertex; 3]),
    Quad([FaceVertex; 4]),
}

impl AbsDiffEq for FaceVertices {
    type Epsilon = f32;

    fn default_epsilon() -> Self::Epsilon {
        f32::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        match (self, other) {
            (FaceVertices::Triangle(lhs), FaceVertices::Triangle(rhs)) => {
                lhs[0].position.abs_diff_eq(&rhs[0].position, epsilon)
                    && lhs[1].position.abs_diff_eq(&rhs[1].position, epsilon)
                    && lhs[2].position.abs_diff_eq(&rhs[2].position, epsilon)
            }
            (FaceVertices::Quad(lhs), FaceVertices::Quad(rhs)) => {
                lhs[0].position.abs_diff_eq(&rhs[0].position, epsilon)
                    && lhs[1].position.abs_diff_eq(&rhs[1].position, epsilon)
                    && lhs[2].position.abs_diff_eq(&rhs[2].position, epsilon)
                    && lhs[3].position.abs_diff_eq(&rhs[3].position, epsilon)
            }
            (_, _) => false,
        }
    }
}

const TRIANGLE_INDEX_ORDER: &[usize] = &[0, 1, 2];
const QUAD_INDEX_ORDER: &[usize] = &[0, 1, 2, 2, 3, 0];

struct FaceIterator<'a> {
    face: &'a FaceVertices,
    iterator: Box<dyn Iterator<Item = &'static usize>>,
}

impl<'a> Iterator for FaceIterator<'a> {
    type Item = &'a FaceVertex;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iterator.next() {
            Some(e) => Some(match self.face {
                FaceVertices::Triangle(v) => &v[*e],
                FaceVertices::Quad(v) => &v[*e],
            }),
            None => None,
        }
    }
}

impl FaceVertices {
    pub fn triangles(&self, reverse: bool) -> FaceIterator {
        let order = match self {
            FaceVertices::Triangle(_) => TRIANGLE_INDEX_ORDER,
            FaceVertices::Quad(_) => QUAD_INDEX_ORDER,
        };

        let iterator: Box<dyn Iterator<Item = &'static usize>> = if reverse {
            Box::new(order.iter().rev())
        } else {
            Box::new(order.iter())
        };

        FaceIterator {
            face: self,
            iterator,
        }
    }

    pub fn query(&self, index: usize) -> &FaceVertex {
        match self {
            FaceVertices::Triangle(a) => &a[TRIANGLE_INDEX_ORDER[index]],
            FaceVertices::Quad(a) => &a[QUAD_INDEX_ORDER[index]],
        }
    }

    pub fn query_mut(&mut self, index: usize) -> &mut FaceVertex {
        match self {
            FaceVertices::Triangle(a) => a.get_mut(TRIANGLE_INDEX_ORDER[index]).unwrap(),
            FaceVertices::Quad(a) => a.get_mut(QUAD_INDEX_ORDER[index]).unwrap(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Face {
    vertices: FaceVertices,
    winding: Winding,
}

#[derive(Debug)]
struct Adjacency {
    pub faces: Vec<(Rc<RefCell<Face>>, usize)>,
}

impl Adjacency {
    pub fn new() -> Adjacency {
        Adjacency { faces: Vec::new() }
    }

    pub fn add(&mut self, face: Rc<RefCell<Face>>, index: usize) {
        self.faces.push((Rc::clone(&face), index));
    }
}

fn calculate_normal(v1: &Vector3, v2: &Vector3, v3: &Vector3) -> Vector3 {
    let normal = (v2 - v3).cross(v2 - v1).normalize();
    if normal.x.is_nan() || normal.y.is_nan() || normal.z.is_nan() {
        Vector3::new(0.0, 0.0, 0.0)
    } else {
        normal
    }
}

#[derive(Debug)]
struct MeshBuilder {
    pub faces: HashMap<MeshGroupKey, Vec<Rc<RefCell<Face>>>>,
    adjacencies: Vec<Rc<RefCell<Adjacency>>>,
    point_cloud: KdTree<f32, Rc<RefCell<Adjacency>>, [f32; 3]>,
}

impl MeshBuilder {
    pub fn new() -> MeshBuilder {
        MeshBuilder {
            faces: HashMap::new(),
            adjacencies: Vec::new(),
            point_cloud: KdTree::new(3),
        }
    }

    pub fn add(&mut self, group_key: &MeshGroupKey, face: Rc<RefCell<Face>>) {
        let list = self.faces.entry(group_key.clone()).or_insert_with(Vec::new);
        list.push(face.clone());

        for (index, vertex) in face.borrow().vertices.triangles(false).enumerate() {
            let r: &[f32; 3] = vertex.position.as_ref();
            let nearest = match self.point_cloud.iter_nearest_mut(r, &squared_euclidean) {
                Ok(mut v) => match v.next() {
                    Some(vv) => {
                        if vv.0 < f32::default_epsilon() {
                            Some(vv.1)
                        } else {
                            None
                        }
                    }
                    None => None,
                },
                Err(_) => None,
            };

            match nearest {
                Some(e) => {
                    e.borrow_mut().add(Rc::clone(&face), index);
                }
                None => {
                    let adjacency = Rc::new(RefCell::new(Adjacency::new()));
                    adjacency.borrow_mut().add(Rc::clone(&face), index);
                    self.adjacencies.push(Rc::clone(&adjacency));
                    self.point_cloud
                        .add(*vertex.position.as_ref(), adjacency)
                        .unwrap();
                }
            };
        }
    }

    pub fn smooth_normals(&mut self) {
        for adjacency in self.adjacencies.iter() {
            let adjacency = adjacency.borrow_mut();
            let length = adjacency.faces.len();
            let mut flags = vec![false; length];
            let mut marked = Vec::with_capacity(length);
            loop {
                let mut ops = 0;

                for i in 0..length {
                    if !flags[i] {
                        marked.clear();
                        marked.push(i);
                        flags[i] = true;
                        let (face, index) = &adjacency.faces[i];
                        let base_normal = face.borrow().vertices.query(*index).normal;
                        let mut smoothed_normal = base_normal;
                        for (j, flag) in flags.iter_mut().enumerate() {
                            if i != j {
                                let (face, index) = &adjacency.faces[j];
                                let c_normal = face.borrow().vertices.query(*index).normal;
                                let angle = base_normal.angle(c_normal);
                                if angle.0 < f32::default_epsilon() {
                                    *flag = true;
                                }
                                if angle < NORMAL_BLEND_THRESHOLD {
                                    ops += 1;
                                    *flag = true;
                                    marked.push(j);
                                    smoothed_normal += c_normal;
                                }
                            }
                        }

                        if !marked.is_empty() {
                            smoothed_normal /= marked.len() as f32;
                        }

                        for j in marked.iter() {
                            let (face, index) = &adjacency.faces[*j];
                            face.borrow_mut().vertices.query_mut(*index).normal = smoothed_normal;
                        }
                    }
                }

                if ops == 0 {
                    break;
                }
            }
        }
    }

    pub fn bake(&self, builder: &mut PartBufferBundleBuilder, bounding_box: &mut BoundingBox3) {
        let mut bounding_box_min = None;
        let mut bounding_box_max = None;

        for (group_key, faces) in self.faces.iter() {
            let mut vertex_indices = vec![];
            let mut normal_indices = vec![];

            for face in faces.iter() {
                for vertex in face.borrow().vertices.triangles(false) {
                    match bounding_box_min {
                        None => {
                            bounding_box_min = Some(vertex.position);
                        }
                        Some(ref mut e) => {
                            if e.x > vertex.position.x {
                                e.x = vertex.position.x;
                            }
                            if e.y > vertex.position.y {
                                e.y = vertex.position.y;
                            }
                            if e.z > vertex.position.z {
                                e.z = vertex.position.z;
                            }
                        }
                    }
                    match bounding_box_max {
                        None => {
                            bounding_box_max = Some(vertex.position);
                        }
                        Some(ref mut e) => {
                            if e.x < vertex.position.x {
                                e.x = vertex.position.x;
                            }
                            if e.y < vertex.position.y {
                                e.y = vertex.position.y;
                            }
                            if e.z < vertex.position.z {
                                e.z = vertex.position.z;
                            }
                        }
                    }

                    vertex_indices.push(builder.vertex_buffer_builder.add(vertex.position));
                    normal_indices.push(builder.vertex_buffer_builder.add(vertex.normal));
                }
            }

            if let Some(mesh_buffer) = builder.query_mesh(group_key) {
                mesh_buffer.add_indices(vertex_indices, normal_indices);
            } else {
                println!("Skipping unknown color group_key {:?}", group_key);
            }
        }

        if let Some(bounding_box_min) = bounding_box_min {
            if let Some(bounding_box_max) = bounding_box_max {
                bounding_box.update_point(&bounding_box_min);
                bounding_box.update_point(&bounding_box_max);
            }
        }
    }
}

struct PartBaker<'a> {
    resolutions: &'a ResolutionResult,

    metadata: PartMetadata,
    builder: PartBufferBundleBuilder,
    mesh_builder: MeshBuilder,
    color_stack: Vec<ColorReference>,
}

impl<'a> PartBaker<'a> {
    pub fn traverse<M: Deref<Target = MultipartDocument>>(
        &mut self,
        document: &Document,
        parent: M,
        matrix: Matrix4,
        cull: bool,
        invert: bool,
        local: bool,
    ) {
        let mut local_cull = true;
        let mut winding = Winding::Ccw;
        let bfc_certified = document.bfc.is_certified().unwrap_or(true);
        let mut invert_next = false;

        if bfc_certified {
            winding = match document.bfc.get_winding() {
                Some(e) => e,
                None => Winding::Ccw,
            } ^ invert;
        }

        for cmd in document.commands.iter() {
            match cmd {
                Command::PartReference(cmd) => {
                    let matrix = matrix * cmd.matrix;
                    let invert_child = if cmd.matrix.determinant() < -f32::default_epsilon() {
                        invert == invert_next
                    } else {
                        invert != invert_next
                    };

                    let cull_next = if bfc_certified {
                        cull && local_cull
                    } else {
                        false
                    };

                    let color: ColorReference = match &cmd.color {
                        ColorReference::Current => self.color_stack.last().unwrap().clone(),
                        e => e.clone(),
                    };

                    if let Some(part) = parent.get_subpart(&cmd.name) {
                        self.color_stack.push(color);
                        self.traverse(part, &*parent, matrix, cull_next, invert_child, local);
                        self.color_stack.pop();
                    } else if let Some((document, local)) = self.resolutions.query(&cmd.name, local)
                    {
                        self.color_stack.push(color);
                        self.traverse(
                            &document.body,
                            Arc::clone(&document),
                            matrix,
                            cull_next,
                            invert_child,
                            local,
                        );
                        self.color_stack.pop();
                    }

                    invert_next = false;
                }
                Command::Line(cmd) => {
                    let top = self.color_stack.last().unwrap();

                    self.builder.edges.add(
                        &mut self.builder.vertex_buffer_builder,
                        (matrix * cmd.a).truncate(),
                        &cmd.color,
                        top,
                    );
                    self.builder.edges.add(
                        &mut self.builder.vertex_buffer_builder,
                        (matrix * cmd.b).truncate(),
                        &cmd.color,
                        top,
                    );
                }
                Command::OptionalLine(cmd) => {
                    let top = self.color_stack.last().unwrap();

                    self.builder.optional_edges.add(
                        &mut self.builder.vertex_buffer_builder,
                        (matrix * cmd.a).truncate(),
                        (matrix * cmd.b).truncate(),
                        (matrix * cmd.c).truncate(),
                        (matrix * cmd.d).truncate(),
                        &cmd.color,
                        top,
                    );
                }
                Command::Triangle(cmd) => {
                    let color = match &cmd.color {
                        ColorReference::Current => self.color_stack.last().unwrap(),
                        e => e,
                    };

                    let face = match winding {
                        Winding::Ccw => {
                            let v1 = (matrix * cmd.a).truncate();
                            let v2 = (matrix * cmd.b).truncate();
                            let v3 = (matrix * cmd.c).truncate();
                            let normal = calculate_normal(&v1, &v2, &v3);
                            Face {
                                vertices: FaceVertices::Triangle([
                                    FaceVertex {
                                        position: v1,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal,
                                    },
                                ]),
                                winding: Winding::Ccw,
                            }
                        }
                        Winding::Cw => {
                            let v1 = (matrix * cmd.c).truncate();
                            let v2 = (matrix * cmd.b).truncate();
                            let v3 = (matrix * cmd.a).truncate();
                            let normal = calculate_normal(&v1, &v2, &v3);
                            Face {
                                vertices: FaceVertices::Triangle([
                                    FaceVertex {
                                        position: v1,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal,
                                    },
                                ]),
                                winding: Winding::Cw,
                            }
                        }
                    };

                    let category = MeshGroupKey {
                        color_ref: color.clone(),
                        bfc: if bfc_certified {
                            cull && local_cull
                        } else {
                            false
                        },
                    };

                    self.mesh_builder
                        .add(&category, Rc::new(RefCell::new(face)));
                }
                Command::Quad(cmd) => {
                    let color = match &cmd.color {
                        ColorReference::Current => self.color_stack.last().unwrap(),
                        e => e,
                    };

                    let face = match winding {
                        Winding::Ccw => {
                            let v1 = (matrix * cmd.a).truncate();
                            let v2 = (matrix * cmd.b).truncate();
                            let v3 = (matrix * cmd.c).truncate();
                            let v4 = (matrix * cmd.d).truncate();
                            let normal = calculate_normal(&v1, &v2, &v3);
                            Face {
                                vertices: FaceVertices::Quad([
                                    FaceVertex {
                                        position: v1,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v4,
                                        normal,
                                    },
                                ]),
                                winding: Winding::Ccw,
                            }
                        }
                        Winding::Cw => {
                            let v1 = (matrix * cmd.d).truncate();
                            let v2 = (matrix * cmd.c).truncate();
                            let v3 = (matrix * cmd.b).truncate();
                            let v4 = (matrix * cmd.a).truncate();
                            let normal = calculate_normal(&v1, &v2, &v3);
                            Face {
                                vertices: FaceVertices::Quad([
                                    FaceVertex {
                                        position: v1,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal,
                                    },
                                    FaceVertex {
                                        position: v4,
                                        normal,
                                    },
                                ]),
                                winding: Winding::Cw,
                            }
                        }
                    };

                    let category = MeshGroupKey {
                        color_ref: color.clone(),
                        bfc: if bfc_certified {
                            cull && local_cull
                        } else {
                            false
                        },
                    };

                    self.mesh_builder
                        .add(&category, Rc::new(RefCell::new(face)));
                }
                Command::Meta(cmd) => {
                    if let Meta::Bfc(statement) = cmd {
                        match statement {
                            BfcStatement::InvertNext => {
                                invert_next = true;
                            }
                            BfcStatement::NoClip => {
                                local_cull = false;
                            }
                            BfcStatement::Clip(w) => {
                                local_cull = true;
                                if let Some(w) = w {
                                    winding = w ^ invert;
                                }
                            }
                            BfcStatement::Winding(w) => {
                                winding = w ^ invert;
                            }
                        }
                    }
                }
            };
        }
    }

    pub fn bake(mut self) -> Part {
        let mut bounding_box = BoundingBox3::zero();
        self.mesh_builder.smooth_normals();
        self.mesh_builder.bake(&mut self.builder, &mut bounding_box);

        Part::new(
            self.metadata,
            self.builder.build(),
            bounding_box,
            Vector3::new(0.0, 0.0, 0.0),
        )
    }

    pub fn new(metadata: PartMetadata, resolutions: &'a ResolutionResult) -> Self {
        let mut mb = PartBaker {
            resolutions,

            metadata,
            builder: PartBufferBundleBuilder::default(),
            mesh_builder: MeshBuilder::new(),
            color_stack: Vec::new(),
        };

        mb.color_stack.push(ColorReference::Current);

        mb
    }
}

pub fn bake_part_from_multipart_document<D: Deref<Target = MultipartDocument>>(
    document: D,
    resolutions: &ResolutionResult,
    local: bool,
) -> Part {
    let mut baker = PartBaker::new(PartMetadata::from(&document.body), resolutions);

    baker.traverse(
        &document.body,
        &*document,
        Matrix4::identity(),
        true,
        false,
        local,
    );
    baker.bake()
}

pub fn bake_part_from_document(
    document: &Document,
    resolutions: &ResolutionResult,
    local: bool,
) -> Part {
    let mut baker = PartBaker::new(document.into(), resolutions);

    baker.traverse(
        document,
        // NOTE: Workaround as it's a bit tricky to pass parent in Option<T> form
        &MultipartDocument {
            body: Default::default(),
            subparts: HashMap::new(),
        },
        Matrix4::identity(),
        true,
        false,
        local,
    );
    baker.bake()
}
