use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::f32;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;
use std::vec::Vec;

use approx::{abs_diff_eq, AbsDiffEq};
use cgmath::{InnerSpace, SquareMatrix};
use kdtree::distance::squared_euclidean;
use kdtree::KdTree;
use ldraw::color::{ColorReference, MaterialRegistry};
use ldraw::document::Document;
use ldraw::elements::{BfcStatement, Command, Meta};
use ldraw::library::{ResolutionMap, ResolutionResult};
use ldraw::{Matrix4, NormalizedAlias, Vector3, Vector4, Winding};

const NORMAL_BLEND_THRESHOLD: f32 = f32::consts::FRAC_PI_4;

#[derive(Clone, Debug)]
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

#[derive(Clone, Copy, Debug)]
pub struct IndexBound(pub usize, pub usize);

#[derive(Clone, Debug)]
pub struct BufferIndex(pub HashMap<GroupKey, IndexBound>);

impl BufferIndex {

    pub fn new() -> BufferIndex {
        BufferIndex(HashMap::new())
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
    pub fn count(&self) -> usize {
        match self {
            FaceVertices::Triangle(_) => 3,
            FaceVertices::Quad(_) => 4,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Vector3> {
        match self {
            FaceVertices::Triangle(a) => a.iter(),
            FaceVertices::Quad(a) => a.iter(),
        }
    }

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

struct Adjacency {
    pub position: Vector3,
    pub faces: Vec<Face>,
    pub index: usize,
}

impl<'a> Adjacency {
    pub fn new(position: &Vector3, index: usize) -> Adjacency {
        Adjacency {
            position: *position,
            faces: Vec::new(),
            index,
        }
    }

    pub fn query(
        &'a self,
        v: &'a Vector3,
        exclude: &'a Face,
    ) -> impl Iterator<Item = &'a Face> + 'a {
        self.faces
            .iter()
            .filter(move |&i| i.vertices.contains(v) && i != exclude)
    }
}

#[derive(Clone, Debug, Default)]
pub struct EdgeBuffer {
    pub vertices: Vec<f32>,
    pub colors: Vec<f32>,
}

impl EdgeBuffer {
    pub fn new() -> EdgeBuffer {
        EdgeBuffer {
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

#[derive(Debug)]
pub struct MeshBuffer {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
}

impl MeshBuffer {
    pub fn new() -> MeshBuffer {
        MeshBuffer {
            vertices: Vec::new(),
            normals: Vec::new(),
        }
    }
    
    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }
}

#[derive(Debug)]
struct MeshBuilder {
    pub faces: HashMap<GroupKey, Vec<Face>>,
}

impl MeshBuilder {
    pub fn new() -> MeshBuilder {
        MeshBuilder { faces: HashMap::new() }
    }

    pub fn add(&mut self, group_key: &GroupKey, face: Face) {
        let list = self.faces.entry(group_key.clone()).or_insert(Vec::new());
        list.push(face);
    }

    pub fn bake(&self) -> HashMap<GroupKey, MeshBuffer>  {
        let mut mesh_group = HashMap::new();

        for (group_key, faces) in self.faces.iter() {
            let mut vertices = Vec::new();
            let mut normals = Vec::new();
            for face in faces.iter() {
                let normal = face.vertices.normal();

                for vertex in face.vertices.triangles(false) {
                    vertices.push(vertex.x);
                    vertices.push(vertex.y);
                    vertices.push(vertex.z);
                    normals.push(normal.x);
                    normals.push(normal.y);
                    normals.push(normal.z);
                }
            }

            mesh_group.insert(group_key.clone(), MeshBuffer { vertices, normals });
        }

        mesh_group
    }
}

pub struct ModelBuilder<'a, 'b, T> {
    materials: &'a MaterialRegistry,
    resolutions: &'b ResolutionMap<'b, T>,

    merge_buffer: BakedModel,
    mesh_builder: MeshBuilder,
    edges: EdgeBuffer,
    color_stack: Vec<ColorReference>,
    point_cloud: KdTree<f32, Adjacency, [f32; 3]>,
}

impl<'a, 'b, T: Clone> ModelBuilder<'a, 'b, T> {
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
                None => Winding::Ccw
            } ^ invert;
        }

        for cmd in document.commands.iter() {
            match cmd {
                Command::PartReference(cmd) => {
                    let matrix = matrix * cmd.matrix;
                    let invert_child = if matrix.determinant() < -f32::default_epsilon() {
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

                    match self.resolutions.get(cmd) {
                        Some(ResolutionResult::Subpart(part)) => {
                            self.color_stack.push(color);
                            self.traverse(
                                part,
                                matrix,
                                cull_next,
                                invert_child,
                            );
                            self.color_stack.pop();
                        }
                        Some(ResolutionResult::Associated(part)) => {
                            self.color_stack.push(color);
                            self.traverse(
                                &Rc::clone(part),
                                matrix,
                                cull_next,
                                invert_child,
                            );
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
                        }
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
                        }
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

    pub fn bake(&self) -> BakedModel {
        let mut model = BakedModel {
            mesh: MeshBuffer::new(),
            mesh_index: BufferIndex::new(),
            edges: self.edges.clone(),
        };

        let mut mesh_groups = self.mesh_builder.bake();

        let mut index = 0;
        for (group, mesh) in mesh_groups.iter() {
            model.mesh.vertices.extend(&mesh.vertices);
            model.mesh.normals.extend(&mesh.normals);
            model.mesh_index.0.insert(
                group.clone(), IndexBound(index / 3, (index + mesh.vertices.len()) / 3)
            );

            index += mesh.vertices.len();
        }

        model
            .edges
            .vertices
            .extend(&self.merge_buffer.edges.vertices);
        model.edges.colors.extend(&self.merge_buffer.edges.colors);

        model
    }

    pub fn visualize_normals(&self, scale: f32) -> EdgeBuffer {
        let mut buffer = EdgeBuffer::default();

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

    pub fn new(
        materials: &'a MaterialRegistry,
        resolutions: &'b ResolutionMap<T>,
    ) -> ModelBuilder<'a, 'b, T> {
        let mut mb = ModelBuilder {
            materials,
            resolutions,

            merge_buffer: BakedModel {
                mesh: MeshBuffer::new(),
                mesh_index: BufferIndex::new(),
                edges: EdgeBuffer::new(),
            },
            mesh_builder: MeshBuilder::new(),
            edges: EdgeBuffer::new(),
            color_stack: Vec::new(),
            point_cloud: KdTree::new(3),
        };

        mb.color_stack.push(ColorReference::Current);

        mb
    }
}

#[derive(Debug)]
pub struct BakedModel {
    pub mesh: MeshBuffer,
    pub mesh_index: BufferIndex,
    pub edges: EdgeBuffer,
}

pub fn bake_model<'a, T: Clone, S: BuildHasher>(
    materials: &MaterialRegistry,
    resolution: &ResolutionMap<'a, T>,
    document: &Document,
) -> BakedModel {
    let mut builder = ModelBuilder::new(materials, resolution);

    builder.traverse(&document, Matrix4::identity(), true, false);
    builder.bake()
}
