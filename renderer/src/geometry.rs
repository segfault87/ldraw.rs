use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::f32;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;
use std::vec::Vec;

use approx::{abs_diff_eq, AbsDiffEq};
use cgmath::{InnerSpace, Rad, SquareMatrix};
use kdtree::distance::squared_euclidean;
use kdtree::KdTree;
use ldraw::color::{ColorReference, MaterialRegistry};
use ldraw::document::Document;
use ldraw::elements::{BfcStatement, Command, Meta};
use ldraw::library::{ResolutionMap, ResolutionResult};
use ldraw::{AliasType, Matrix4, NormalizedAlias, Vector3, Vector4, Winding};
use serde::{Deserialize, Serialize};

use crate::BoundingBox;

const NORMAL_BLEND_THRESHOLD: Rad<f32> = Rad(f32::consts::FRAC_PI_6);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GroupKey {
    pub color_ref: ColorReference,
    pub bfc: bool,
}

impl Hash for GroupKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color_ref.code().hash(state);
        self.bfc.hash(state);
    }
}

impl PartialOrd for GroupKey {
    fn partial_cmp(&self, other: &GroupKey) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GroupKey {
    fn cmp(&self, other: &GroupKey) -> Ordering {
        let lhs_semitransparent = match &self.color_ref {
            ColorReference::Material(m) => m.is_semi_transparent(),
            _ => false,
        };
        let rhs_semitransparent = match &other.color_ref {
            ColorReference::Material(m) => m.is_semi_transparent(),
            _ => false,
        };

        match (lhs_semitransparent, rhs_semitransparent) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            (_, _) => (),
        };

        match self.color_ref.code().cmp(&other.color_ref.code()) {
            Ordering::Equal => self.bfc.cmp(&other.bfc),
            e => e,
        }
    }
}

impl Eq for GroupKey {}

impl PartialEq for GroupKey {
    fn eq(&self, other: &GroupKey) -> bool {
        self.color_ref.code() == other.color_ref.code() && self.bfc == other.bfc
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct IndexBound(pub usize, pub usize);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BufferIndex(pub HashMap<GroupKey, IndexBound>);

impl BufferIndex {
    pub fn new() -> BufferIndex {
        BufferIndex(HashMap::new())
    }

    pub fn resolve(&mut self, materials: &MaterialRegistry) {
        let mut new = HashMap::new();
        for (k, v) in self.0.iter() {
            new.insert(
                GroupKey {
                    color_ref: ColorReference::resolve(k.color_ref.code(), materials),
                    bfc: k.bfc,
                },
                v.clone(),
            );
        }

        self.0.clear();
        self.0.extend(new);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum FaceVertices {
    Triangle([Vector3; 3]),
    Quad([Vector3; 4]),
}

#[derive(Clone, Debug, PartialEq)]
struct Face {
    vertices: FaceVertices,
    winding: Winding,
}

impl AbsDiffEq for FaceVertices {
    type Epsilon = f32;

    fn default_epsilon() -> Self::Epsilon {
        f32::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        match (self, other) {
            (FaceVertices::Triangle(lhs), FaceVertices::Triangle(rhs)) => {
                (lhs[0].abs_diff_eq(&rhs[0], epsilon)
                    && lhs[1].abs_diff_eq(&rhs[1], epsilon)
                    && lhs[2].abs_diff_eq(&rhs[2], epsilon))
            }
            (FaceVertices::Quad(lhs), FaceVertices::Quad(rhs)) => {
                (lhs[0].abs_diff_eq(&rhs[0], epsilon)
                    && lhs[1].abs_diff_eq(&rhs[1], epsilon)
                    && lhs[2].abs_diff_eq(&rhs[2], epsilon)
                    && lhs[3].abs_diff_eq(&rhs[3], epsilon))
            }
            (_, _) => false,
        }
    }
}

impl AsRef<[Vector3]> for FaceVertices {
    fn as_ref(&self) -> &[Vector3] {
        match self {
            FaceVertices::Triangle(v) => v.as_ref(),
            FaceVertices::Quad(v) => v.as_ref(),
        }
    }
}

const TRIANGLE_INDEX_ORDER: &[usize] = &[0, 1, 2];
const QUAD_INDEX_ORDER: &[usize] = &[0, 1, 2, 2, 3, 0];

struct FaceIterator<'a> {
    face: &'a [Vector3],
    iterator: Box<dyn Iterator<Item = &'static usize>>,
}

impl<'a> Iterator for FaceIterator<'a> {
    type Item = &'a Vector3;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iterator.next() {
            Some(e) => Some(&self.face[*e]),
            None => None,
        }
    }
}

impl<'a> FaceVertices {
    pub fn center(&self) -> Vector3 {
        match self {
            FaceVertices::Triangle(a) => (a[0] + a[1] + a[2]) / 3.0,
            FaceVertices::Quad(a) => (a[0] + a[1] + a[2] + a[3]) / 4.0,
        }
    }

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
            face: self.as_ref(),
            iterator,
        }
    }

    pub fn edge(&'a self, index: usize) -> (&'a Vector3, &'a Vector3) {
        match self {
            FaceVertices::Triangle(v) => (&v[index], &v[(index + 1) % 3]),
            FaceVertices::Quad(v) => (&v[index], &v[(index + 1) % 4]),
        }
    }

    pub fn contains(&self, vec: &Vector3) -> bool {
        match self {
            FaceVertices::Triangle(v) => {
                for i in v {
                    if abs_diff_eq!(vec, i) {
                        return true;
                    }
                }
            }
            FaceVertices::Quad(v) => {
                for i in v {
                    if abs_diff_eq!(vec, i) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn normal(&self) -> Vector3 {
        let r = match self {
            FaceVertices::Triangle(v) => v.as_ref(),
            FaceVertices::Quad(v) => v.as_ref(),
        };

        (r[1] - r[2]).cross(r[1] - r[0]).normalize()
    }
}

#[derive(Debug)]
struct Adjacency {
    pub position: Vector3,
    pub faces: Vec<Face>,
}

impl<'a> Adjacency {
    pub fn new(position: &Vector3) -> Adjacency {
        Adjacency {
            position: position.clone(),
            faces: Vec::new(),
        }
    }

    pub fn add(&mut self, face: &Face) {
        self.faces.push(face.clone());
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NativeEdgeBuffer {
    pub vertices: Vec<f32>,
    pub colors: Vec<f32>,
}

impl NativeEdgeBuffer {
    pub fn new() -> NativeEdgeBuffer {
        NativeEdgeBuffer {
            vertices: Vec::new(),
            colors: Vec::new(),
        }
    }

    pub fn add(&mut self, vec: &Vector3, color: &ColorReference, top: &ColorReference) {
        self.vertices.push(vec.x);
        self.vertices.push(vec.y);
        self.vertices.push(vec.z);

        if color.is_current() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.color.into();
                self.colors.push(mv.x);
                self.colors.push(mv.y);
                self.colors.push(mv.z);
            } else {
                self.colors.push(-1.0);
                self.colors.push(-1.0);
                self.colors.push(-1.0);
            }
        } else if color.is_complement() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.edge.into();
                self.colors.push(mv.x);
                self.colors.push(mv.y);
                self.colors.push(mv.z);
            } else {
                self.colors.push(-2.0);
                self.colors.push(-2.0);
                self.colors.push(-2.0);
            }
        } else if let Some(c) = color.get_material() {
            let mv: Vector4 = c.color.into();
            self.colors.push(mv.x);
            self.colors.push(mv.y);
            self.colors.push(mv.z);
        } else {
            self.colors.push(0.0);
            self.colors.push(0.0);
            self.colors.push(0.0);
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NativeMeshBuffer {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
}

impl NativeMeshBuffer {
    pub fn new() -> NativeMeshBuffer {
        NativeMeshBuffer {
            vertices: Vec::new(),
            normals: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NativeBuffer {
    pub mesh: NativeMeshBuffer,
    pub edges: NativeEdgeBuffer,
}

pub type FeatureMap = HashMap<NormalizedAlias, Vec<(ColorReference, Matrix4)>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct BakedModel<B> {
    pub buffer: B,
    pub mesh_index: BufferIndex,
    pub features: FeatureMap,
    pub bounding_box: BoundingBox,
    pub rotation_center: Vector3,
}

impl<B> BakedModel<B> {
    pub fn new(
        buffer: B,
        index: BufferIndex,
        features: FeatureMap,
        bounding_box: BoundingBox,
        rotation_center: &Vector3,
    ) -> BakedModel<B> {
        BakedModel {
            buffer,
            mesh_index: index,
            features,
            bounding_box,
            rotation_center: rotation_center.clone(),
        }
    }
}

pub type NativeBakedModel = BakedModel<NativeBuffer>;

#[derive(Debug)]
struct MeshBuilder {
    pub faces: HashMap<GroupKey, Vec<Face>>,
    point_cloud: KdTree<f32, Adjacency, [f32; 3]>,
}

impl MeshBuilder {
    pub fn new() -> MeshBuilder {
        MeshBuilder {
            faces: HashMap::new(),
            point_cloud: KdTree::new(3),
        }
    }

    pub fn add(&mut self, group_key: &GroupKey, face: Face) {
        let list = self.faces.entry(group_key.clone()).or_insert(Vec::new());
        list.push(face.clone());

        for vertex in face.vertices.triangles(false) {
            let r: &[f32; 3] = vertex.as_ref();
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
                    e.add(&face);
                }
                None => {
                    let mut adjacency = Adjacency::new(&vertex);
                    adjacency.add(&face);
                    self.point_cloud.add(*vertex.as_ref(), adjacency).unwrap();
                }
            };
        }
    }

    pub fn bake(&self) -> HashMap<GroupKey, (NativeMeshBuffer, BoundingBox)> {
        let mut mesh_group = HashMap::new();

        let mut bounding_box_min = None;
        let mut bounding_box_max = None;

        for (group_key, faces) in self.faces.iter() {
            let mut vertices = Vec::new();
            let mut normals = Vec::new();
            for face in faces.iter() {
                let normal = face.vertices.normal();

                for vertex in face.vertices.triangles(false) {
                    match bounding_box_min {
                        None => {
                            bounding_box_min = Some(vertex.clone());
                        }
                        Some(ref mut e) => {
                            if e.x > vertex.x {
                                e.x = vertex.x;
                            }
                            if e.y > vertex.y {
                                e.y = vertex.y;
                            }
                            if e.z > vertex.z {
                                e.z = vertex.z;
                            }
                        }
                    }
                    match bounding_box_max {
                        None => {
                            bounding_box_max = Some(vertex.clone());
                        }
                        Some(ref mut e) => {
                            if e.x < vertex.x {
                                e.x = vertex.x;
                            }
                            if e.y < vertex.y {
                                e.y = vertex.y;
                            }
                            if e.z < vertex.z {
                                e.z = vertex.z;
                            }
                        }
                    }

                    vertices.push(vertex.x);
                    vertices.push(vertex.y);
                    vertices.push(vertex.z);

                    let r: &[f32; 3] = vertex.as_ref();
                    let adjacent_faces = match self.point_cloud.iter_nearest(r, &squared_euclidean)
                    {
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

                    match adjacent_faces {
                        Some(v) => {
                            let mut normal = normal.clone();
                            for face in v.faces.iter() {
                                let fnormal = face.vertices.normal();
                                if normal.angle(fnormal) < NORMAL_BLEND_THRESHOLD {
                                    normal = (normal + fnormal) * 0.5;
                                }
                            }
                            normal = normal.normalize();
                            normals.push(normal.x);
                            normals.push(normal.y);
                            normals.push(normal.z);
                        }
                        None => {
                            normals.push(normal.x);
                            normals.push(normal.y);
                            normals.push(normal.z);
                        }
                    };
                }
            }

            mesh_group.insert(
                group_key.clone(),
                (
                    NativeMeshBuffer { vertices, normals },
                    BoundingBox::new(
                        &bounding_box_min.unwrap_or(Vector3::new(0.0, 0.0, 0.0)),
                        &bounding_box_max.unwrap_or(Vector3::new(0.0, 0.0, 0.0)),
                    ),
                ),
            );
        }

        mesh_group
    }
}

pub struct ModelBuilder<'a, T> {
    resolutions: &'a ResolutionMap<'a, T>,

    mesh_builder: MeshBuilder,
    edges: NativeEdgeBuffer,
    color_stack: Vec<ColorReference>,
    features: FeatureMap,

    enabled_features: HashSet<NormalizedAlias>,
}

impl<'a, T: AliasType> ModelBuilder<'a, T> {
    pub fn traverse<D: Deref<Target = Document>>(
        &mut self,
        document: &D,
        matrix: Matrix4,
        cull: bool,
        invert: bool,
    ) {
        let mut local_cull = true;
        let mut winding = Winding::Ccw;
        let bfc_certified = match document.bfc.is_certified() {
            Some(e) => e,
            None => true,
        };
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

                    if self.enabled_features.contains(&cmd.name) {
                        (*self.features.entry(cmd.name.clone()).or_insert(Vec::new()))
                            .push((color.clone(), matrix.clone()));
                        invert_next = false;
                        continue;
                    }

                    match self.resolutions.get(cmd) {
                        Some(ResolutionResult::Subpart(part)) => {
                            self.color_stack.push(color);
                            self.traverse(part, matrix, cull_next, invert_child);
                            self.color_stack.pop();
                        }
                        Some(ResolutionResult::Associated(part)) => {
                            self.color_stack.push(color);
                            self.traverse(&Rc::clone(part), matrix, cull_next, invert_child);
                            self.color_stack.pop();
                        }
                        _ => (),
                    };

                    invert_next = false;
                }
                Command::Line(cmd) => {
                    let top = self.color_stack.last().unwrap();

                    self.edges
                        .add(&(matrix * cmd.a).truncate(), &cmd.color, top);
                    self.edges
                        .add(&(matrix * cmd.b).truncate(), &cmd.color, top);
                }
                Command::Triangle(cmd) => {
                    let color = match &cmd.color {
                        ColorReference::Current => self.color_stack.last().unwrap(),
                        e => e,
                    };

                    let face = match winding {
                        Winding::Ccw => Face {
                            vertices: FaceVertices::Triangle([
                                (matrix * cmd.a).truncate(),
                                (matrix * cmd.b).truncate(),
                                (matrix * cmd.c).truncate(),
                            ]),
                            winding: Winding::Ccw,
                        },
                        Winding::Cw => Face {
                            vertices: FaceVertices::Triangle([
                                (matrix * cmd.c).truncate(),
                                (matrix * cmd.b).truncate(),
                                (matrix * cmd.a).truncate(),
                            ]),
                            winding: Winding::Cw,
                        },
                    };

                    let category = GroupKey {
                        color_ref: color.clone(),
                        bfc: if bfc_certified {
                            cull && local_cull
                        } else {
                            false
                        },
                    };

                    self.mesh_builder.add(&category, face);
                }
                Command::Quad(cmd) => {
                    let color = match &cmd.color {
                        ColorReference::Current => self.color_stack.last().unwrap(),
                        e => e,
                    };

                    let face = match winding {
                        Winding::Ccw => Face {
                            vertices: FaceVertices::Quad([
                                (matrix * cmd.a).truncate(),
                                (matrix * cmd.b).truncate(),
                                (matrix * cmd.c).truncate(),
                                (matrix * cmd.d).truncate(),
                            ]),
                            winding: Winding::Ccw,
                        },
                        Winding::Cw => Face {
                            vertices: FaceVertices::Quad([
                                (matrix * cmd.d).truncate(),
                                (matrix * cmd.c).truncate(),
                                (matrix * cmd.b).truncate(),
                                (matrix * cmd.a).truncate(),
                            ]),
                            winding: Winding::Cw,
                        },
                    };

                    let category = GroupKey {
                        color_ref: color.clone(),
                        bfc: if bfc_certified {
                            cull && local_cull
                        } else {
                            false
                        },
                    };

                    self.mesh_builder.add(&category, face);
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
                _ => (),
            };
        }
    }

    pub fn bake(&self) -> NativeBakedModel {
        let mut built_mesh = NativeMeshBuffer::new();
        let mut built_index = BufferIndex::new();
        let mesh_groups = self.mesh_builder.bake();
        let mut bounding_box = None;
        let mut index = 0;
        for (group, (mesh, sub_bounding_box)) in mesh_groups.iter() {
            match bounding_box {
                None => {
                    bounding_box = Some(sub_bounding_box.clone());
                }
                Some(ref mut e) => {
                    e.update(&sub_bounding_box);
                }
            };

            built_mesh.vertices.extend(&mesh.vertices);
            built_mesh.normals.extend(&mesh.normals);
            built_index.0.insert(
                group.clone(),
                IndexBound(index / 3, (index + mesh.vertices.len()) / 3),
            );

            index += mesh.vertices.len();
        }

        NativeBakedModel::new(
            NativeBuffer {
                mesh: built_mesh,
                edges: self.edges.clone(),
            },
            built_index,
            self.features.clone(),
            bounding_box.unwrap_or(BoundingBox::zero()),
            &Vector3::new(0.0, 0.0, 0.0),
        )
    }

    pub fn visualize_normals(&self, scale: f32) -> NativeEdgeBuffer {
        let mut buffer = NativeEdgeBuffer::default();

        for (group, mesh) in self.mesh_builder.faces.iter() {
            if !group.bfc {
                continue;
            }
            for face in mesh.iter() {
                let normal = face.vertices.normal();
                let c = face.vertices.center();
                buffer.vertices.push(c.x);
                buffer.vertices.push(c.y);
                buffer.vertices.push(c.z);
                let w = c + (normal * scale);
                buffer.vertices.push(w.x);
                buffer.vertices.push(w.y);
                buffer.vertices.push(w.z);
                buffer.colors.push(1.0);
                buffer.colors.push(0.0);
                buffer.colors.push(1.0);
                buffer.colors.push(1.0);
                buffer.colors.push(1.0);
                buffer.colors.push(0.0);
            }
        }

        buffer
    }

    pub fn new(resolutions: &'a ResolutionMap<T>) -> ModelBuilder<'a, T> {
        let mut mb = ModelBuilder {
            resolutions,

            mesh_builder: MeshBuilder::new(),
            edges: NativeEdgeBuffer::new(),
            color_stack: Vec::new(),
            features: HashMap::new(),

            enabled_features: HashSet::new(),
        };

        mb.color_stack.push(ColorReference::Current);

        mb
    }

    pub fn with_feature(mut self, alias: NormalizedAlias) -> Self {
        self.enabled_features.insert(alias);

        self
    }
}

pub fn bake_model<'a, T: AliasType, S: BuildHasher>(
    resolution: &ResolutionMap<'a, T>,
    document: &Document,
) -> NativeBakedModel {
    let mut builder = ModelBuilder::new(resolution);

    builder.traverse(&document, Matrix4::identity(), true, false);
    builder.bake()
}
