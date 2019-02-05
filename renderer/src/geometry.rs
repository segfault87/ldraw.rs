use std::cmp::Ordering;
use std::collections::HashMap;
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
    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }
}

#[derive(Debug)]
struct MeshBuilder {
    faces: Vec<Face>,
}

impl MeshBuilder {
    pub fn new() -> MeshBuilder {
        MeshBuilder { faces: Vec::new() }
    }

    pub fn add(&mut self, face: Face) {
        self.faces.push(face);
    }

    pub fn bake(&self) -> MeshBuffer {
        let mut buffer = MeshBuffer {
            vertices: Vec::new(),
            normals: Vec::new(),
        };

        for face in self.faces.iter() {
            let normal = face.vertices.normal();

            for vertex in face.vertices.triangles(false) {
                buffer.vertices.push(vertex.x);
                buffer.vertices.push(vertex.y);
                buffer.vertices.push(vertex.z);
                buffer.normals.push(normal.x);
                buffer.normals.push(normal.y);
                buffer.normals.push(normal.z);
            }
        }

        buffer
    }
}

pub struct ModelBuilder<'a, 'b, T> {
    materials: &'a MaterialRegistry,
    resolutions: &'b ResolutionMap<'b, T>,

    merge_buffer: BakedModel,
    meshes: HashMap<GroupKey, MeshBuilder>,
    edges: EdgeBuffer,
    color_stack: Vec<ColorReference>,
    point_cloud: KdTree<f32, Adjacency, [f32; 3]>,
}

impl<'a, 'b, T: Clone> ModelBuilder<'a, 'b, T> {
    fn merge(&mut self, model: &BakedModel, matrix: Matrix4, invert: bool, color: &ColorReference) {
        for (group, mesh) in model.meshes.iter() {
            let igroup = GroupKey {
                color_ref: match &group.color_ref {
                    ColorReference::Current => color.clone(),
                    e => e.clone(),
                },
                bfc: group.bfc,
            };

            let len = mesh.vertices.len();
            if len % 9 != 0 && len != mesh.normals.len() {
                panic!("Malformed mesh buffer");
            }

            let target = match self.merge_buffer.meshes.get_mut(&igroup) {
                Some(e) => e,
                None => {
                    self.merge_buffer.meshes.insert(
                        igroup.clone(),
                        MeshBuffer {
                            vertices: Vec::new(),
                            normals: Vec::new(),
                        },
                    );
                    self.merge_buffer.meshes.get_mut(&igroup).unwrap()
                }
            };

            target.vertices.reserve(len);
            target.normals.reserve(len);

            let mut normal_matrix = matrix.clone();
            normal_matrix[3] = Vector4::new(0.0, 0.0, 0.0, 1.0);

            for i in 0..(len / 9) {
                let v1 = matrix
                    * Vector4::new(
                        mesh.vertices[i * 9],
                        mesh.vertices[i * 9 + 1],
                        mesh.vertices[i * 9 + 2],
                        1.0,
                    );
                let v2 = matrix
                    * Vector4::new(
                        mesh.vertices[i * 9 + 3],
                        mesh.vertices[i * 9 + 4],
                        mesh.vertices[i * 9 + 5],
                        1.0,
                    );
                let v3 = matrix
                    * Vector4::new(
                        mesh.vertices[i * 9 + 6],
                        mesh.vertices[i * 9 + 7],
                        mesh.vertices[i * 9 + 8],
                        1.0,
                    );

                let invert = invert ^ (normal_matrix.determinant() < -f32::default_epsilon());

                let n1 = (normal_matrix
                    * Vector4::new(
                        mesh.normals[i * 9],
                        mesh.normals[i * 9 + 1],
                        mesh.normals[i * 9 + 2],
                        1.0,
                    )).truncate().normalize();
                let n2 = (normal_matrix
                    * Vector4::new(
                        mesh.normals[i * 9 + 3],
                        mesh.normals[i * 9 + 4],
                        mesh.normals[i * 9 + 5],
                        1.0,
                    )).truncate().normalize();
                let n3 = (normal_matrix
                    * Vector4::new(
                        mesh.normals[i * 9 + 6],
                        mesh.normals[i * 9 + 7],
                        mesh.normals[i * 9 + 8],
                        1.0,
                    )).truncate().normalize();

                let vertex_order = if invert {
                    [(&v3, &n3), (&v2, &n2), (&v1, &n1)]
                } else {
                    [(&v1, &n1), (&v2, &n2), (&v3, &n3)]
                };

                for (v, n) in vertex_order.iter() {
                    target.vertices.push(v.x);
                    target.vertices.push(v.y);
                    target.vertices.push(v.z);
                    target.normals.push(n.x);
                    target.normals.push(n.y);
                    target.normals.push(n.z);
                }
            }
        }

        let edge_len = model.edges.vertices.len();
        if edge_len % 6 != 0 && edge_len != model.edges.colors.len() {
            panic!("Malformed edge buffer");
        }

        let color_current;
        let color_complement;
        if color.is_current() || color.is_complement() {
            let top = self.color_stack.last().unwrap();
            if top.is_current() || top.is_complement() {
                color_current = Vector4::new(-1.0, -1.0, -1.0, 1.0);
                color_complement = Vector4::new(-2.0, -2.0, -2.0, 1.0);
            } else if let Some(m) = top.get_material() {
                color_current = Vector4::from(&m.color);
                color_complement = Vector4::from(&m.edge);
            } else {
                color_current = Vector4::new(0.0, 0.0, 0.0, 1.0);
                color_complement = Vector4::new(0.0, 0.0, 0.0, 1.0);
            }
        } else if let Some(m) = color.get_material() {
            color_current = Vector4::from(&m.color);
            color_complement = Vector4::from(&m.edge);
        } else {
            color_current = Vector4::new(0.0, 0.0, 0.0, 1.0);
            color_complement = Vector4::new(0.0, 0.0, 0.0, 1.0);
        }

        let edge = &model.edges;
        let target = &mut self.edges;
        target.vertices.reserve(edge_len);
        target.colors.reserve(edge_len);
        for i in 0..(edge_len / 6) {
            let v1 = (matrix
                * Vector4::new(
                    edge.vertices[i * 6],
                    edge.vertices[i * 6 + 1],
                    edge.vertices[i * 6 + 2],
                    1.0,
                ))
            .truncate();
            let v2 = (matrix
                * Vector4::new(
                    edge.vertices[i * 6 + 3],
                    edge.vertices[i * 6 + 4],
                    edge.vertices[i * 6 + 5],
                    1.0,
                ))
            .truncate();
            target.vertices.push(v1.x);
            target.vertices.push(v1.y);
            target.vertices.push(v1.z);
            target.vertices.push(v2.x);
            target.vertices.push(v2.y);
            target.vertices.push(v2.z);

            let c1 = edge.colors[i * 6];
            let c2 = edge.colors[i * 6 + 3];

            if c1 <= -2.0 {
                target.colors.push(color_complement[0]);
                target.colors.push(color_complement[1]);
                target.colors.push(color_complement[2]);
            } else if c1 <= -1.0 {
                target.colors.push(color_current[0]);
                target.colors.push(color_current[1]);
                target.colors.push(color_current[2]);
            } else {
                target.colors.push(c1);
                target.colors.push(edge.colors[i * 6 + 1]);
                target.colors.push(edge.colors[i * 6 + 2]);
            }

            if c2 <= -2.0 {
                target.colors.push(color_complement[0]);
                target.colors.push(color_complement[1]);
                target.colors.push(color_complement[2]);
            } else if c2 <= -1.0 {
                target.colors.push(color_current[0]);
                target.colors.push(color_current[1]);
                target.colors.push(color_current[2]);
            } else {
                target.colors.push(c2);
                target.colors.push(edge.colors[i * 6 + 4]);
                target.colors.push(edge.colors[i * 6 + 5]);
            }
        }
    }

    pub fn traverse<S: BuildHasher, D: Deref<Target = Document>>(
        &mut self,
        baked_subfiles: &mut HashMap<NormalizedAlias, BakedModel, S>,
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
                                baked_subfiles,
                                part,
                                matrix,
                                cull_next,
                                invert_child,
                            );
                            self.color_stack.pop();
                        }
                        Some(ResolutionResult::Associated(part)) => {
                            let subfile = match baked_subfiles.get(&cmd.name) {
                                Some(subfile) => subfile,
                                None => {
                                    let mut builder =
                                        ModelBuilder::new(self.materials, self.resolutions);

                                    builder.traverse(
                                        baked_subfiles,
                                        &Rc::clone(part),
                                        Matrix4::identity(),
                                        true,
                                        false,
                                    );
                                    baked_subfiles.insert(cmd.name.clone(), builder.bake());
                                    baked_subfiles.get(&cmd.name).unwrap()
                                }
                            };

                            self.merge(subfile, matrix, invert, &color);
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

                    match self.meshes.get_mut(&category) {
                        Some(e) => e.add(face),
                        None => {
                            let mut mesh = MeshBuilder::new();
                            mesh.add(face);
                            self.meshes.insert(category, mesh);
                        }
                    };
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

                    match self.meshes.get_mut(&category) {
                        Some(e) => e.add(face),
                        None => {
                            let mut mesh = MeshBuilder::new();
                            mesh.add(face);
                            self.meshes.insert(category, mesh);
                        }
                    };
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
                            BfcStatement::ClipWinding(w) => {
                                local_cull = true;
                                winding = w ^ invert;
                            }
                            BfcStatement::Clip => {
                                local_cull = true;
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
            meshes: HashMap::new(),
            edges: self.edges.clone(),
        };

        for (group, mesh) in self.meshes.iter() {
            model.meshes.insert(group.clone(), mesh.bake());
        }

        for (group, mesh) in self.merge_buffer.meshes.iter() {
            let target = match model.meshes.get_mut(&group) {
                Some(e) => e,
                None => {
                    model.meshes.insert(
                        group.clone(),
                        MeshBuffer {
                            vertices: Vec::new(),
                            normals: Vec::new(),
                        },
                    );
                    model.meshes.get_mut(group).unwrap()
                }
            };

            target.vertices.extend(&mesh.vertices);
            target.normals.extend(&mesh.normals);
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

        for (group, mesh) in self.meshes.iter() {
            if !group.bfc {
                continue;
            }
            for face in mesh.faces.iter() {
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

        for (group, mesh) in self.merge_buffer.meshes.iter() {
            if !group.bfc {
                continue;
            }
            for i in 0..mesh.len() / 3 {
                let v1 = Vector3::new(mesh.vertices[i * 9],
                                      mesh.vertices[i * 9 + 1],
                                      mesh.vertices[i * 9 + 2]);
                let v2 = Vector3::new(mesh.vertices[i * 9 + 3],
                                      mesh.vertices[i * 9 + 4],
                                      mesh.vertices[i * 9 + 5]);
                let v3 = Vector3::new(mesh.vertices[i * 9 + 6],
                                      mesh.vertices[i * 9 + 7],
                                      mesh.vertices[i * 9 + 8]);
                let n1 = Vector3::new(mesh.normals[i * 9],
                                      mesh.normals[i * 9 + 1],
                                      mesh.normals[i * 9 + 2]).normalize();
                let n2 = Vector3::new(mesh.normals[i * 9 + 3],
                                      mesh.normals[i * 9 + 4],
                                      mesh.normals[i * 9 + 5]).normalize();
                let n3 = Vector3::new(mesh.normals[i * 9 + 6],
                                      mesh.normals[i * 9 + 7],
                                      mesh.normals[i * 9 + 8]).normalize();
                let vc = (v1 + v2 + v3) / 3.0;
                let nc = vc + (n1 + n2 + n3) / 3.0 * scale;

                buffer.vertices.push(vc.x);
                buffer.vertices.push(vc.y);
                buffer.vertices.push(vc.z);
                buffer.vertices.push(nc.x);
                buffer.vertices.push(nc.y);
                buffer.vertices.push(nc.z);
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
                meshes: HashMap::new(),
                edges: EdgeBuffer::new(),
            },
            meshes: HashMap::new(),
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
    pub meshes: HashMap<GroupKey, MeshBuffer>,
    pub edges: EdgeBuffer,
}

pub fn bake_model<'a, T: Clone, S: BuildHasher>(
    materials: &MaterialRegistry,
    resolution: &ResolutionMap<'a, T>,
    baked_subfiles: &mut HashMap<NormalizedAlias, BakedModel, S>,
    document: &Document,
) -> BakedModel {
    let mut builder = ModelBuilder::new(materials, resolution);

    builder.traverse(baked_subfiles, &document, Matrix4::identity(), true, false);
    builder.bake()
}
