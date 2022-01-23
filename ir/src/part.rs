use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    f32,
    fmt::Debug,
    mem,
    ops::Deref,
    rc::Rc,
    sync::Arc,
    vec::Vec,
};

use cgmath::{abs_diff_eq, AbsDiffEq, InnerSpace, Rad, SquareMatrix};
use kdtree::{distance::squared_euclidean, KdTree};
use ldraw::{
    color::{ColorReference, MaterialRegistry},
    document::{Document, MultipartDocument},
    elements::{BfcStatement, Command, Meta},
    library::ResolutionResult,
    Matrix4, PartAlias, Vector3, Vector4, Winding,
};
use serde::{Deserialize, Serialize};

use crate::{geometry::BoundingBox3, MeshGroup};

const NORMAL_BLEND_THRESHOLD: Rad<f32> = Rad(f32::consts::FRAC_PI_6);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MeshBufferBuilder {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
}

impl MeshBufferBuilder {
    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn add(&mut self, vertex: &Vector3, normal: &Vector3) {
        self.vertices.push(vertex.x);
        self.vertices.push(vertex.y);
        self.vertices.push(vertex.z);
        self.normals.push(normal.x);
        self.normals.push(normal.y);
        self.normals.push(normal.z);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EdgeBufferBuilder {
    pub vertices: Vec<f32>,
    pub colors: Vec<f32>,
}

impl EdgeBufferBuilder {
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

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OptionalEdgeBufferBuilder {
    pub vertices: Vec<f32>,
    pub controls_1: Vec<f32>,
    pub controls_2: Vec<f32>,
    pub direction: Vec<f32>,
    pub colors: Vec<f32>,
}

impl OptionalEdgeBufferBuilder {
    pub fn add(
        &mut self,
        v1: &Vector3,
        v2: &Vector3,
        c1: &Vector3,
        c2: &Vector3,
        color: &ColorReference,
        top: &ColorReference,
    ) {
        let d = v2 - v1;

        self.vertices.extend(&[v1.x, v1.y, v1.z, v2.x, v2.y, v2.z]);
        self.controls_1
            .extend(&[c1.x, c1.y, c1.z, c1.x, c1.y, c1.z]);
        self.controls_2
            .extend(&[c2.x, c2.y, c2.z, c2.x, c2.y, c2.z]);
        self.direction.extend(&[d.x, d.y, d.z, d.x, d.y, d.z]);

        if color.is_current() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.color.into();
                self.colors.extend(&[mv.x, mv.y, mv.z, mv.x, mv.y, mv.z]);
            } else {
                self.colors.extend(&[-1.0, -1.0, -1.0, -1.0, -1.0, -1.0]);
            }
        } else if color.is_complement() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.edge.into();
                self.colors.extend(&[mv.x, mv.y, mv.z, mv.x, mv.y, mv.z]);
            } else {
                self.colors.extend(&[-2.0, -2.0, -2.0, -2.0, -2.0, -2.0]);
            }
        } else if let Some(c) = color.get_material() {
            let mv: Vector4 = c.color.into();
            self.colors.extend(&[mv.x, mv.y, mv.z, mv.x, mv.y, mv.z]);
        } else {
            self.colors.extend(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubpartIndex {
    pub start: usize,
    pub span: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PartBufferBuilder {
    pub uncolored_mesh: MeshBufferBuilder,
    pub uncolored_without_bfc_mesh: MeshBufferBuilder,
    pub opaque_meshes: HashMap<MeshGroup, MeshBufferBuilder>,
    pub translucent_meshes: HashMap<MeshGroup, MeshBufferBuilder>,
    pub edges: EdgeBufferBuilder,
    pub optional_edges: OptionalEdgeBufferBuilder,
}

impl PartBufferBuilder {
    pub fn query_mesh<'a>(&'a mut self, group: &MeshGroup) -> Option<&'a mut MeshBufferBuilder> {
        match (&group.color_ref, group.bfc) {
            (ColorReference::Current, true) => Some(&mut self.uncolored_mesh),
            (ColorReference::Current, false) => Some(&mut self.uncolored_without_bfc_mesh),
            (ColorReference::Material(m), _) => {
                let entry = if m.is_translucent() {
                    self.translucent_meshes
                        .entry(group.clone())
                        .or_insert_with(MeshBufferBuilder::default)
                } else {
                    self.opaque_meshes
                        .entry(group.clone())
                        .or_insert_with(MeshBufferBuilder::default)
                };
                Some(entry)
            }
            _ => None,
        }
    }

    pub fn resolve_colors(&mut self, colors: &MaterialRegistry) {
        let keys = self.opaque_meshes.keys().cloned().collect::<Vec<_>>();
        for key in keys.iter() {
            let val = match self.opaque_meshes.remove(key) {
                Some(v) => v,
                None => continue,
            };
            self.opaque_meshes.insert(key.clone_resolved(colors), val);
        }
        let keys = self.translucent_meshes.keys().cloned().collect::<Vec<_>>();
        for key in keys.iter() {
            let val = match self.translucent_meshes.remove(key) {
                Some(v) => v,
                None => continue,
            };
            self.translucent_meshes
                .insert(key.clone_resolved(colors), val);
        }
    }
}

pub type FeatureMap = HashMap<PartAlias, Vec<(ColorReference, Matrix4)>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct PartBuilder {
    pub part_builder: PartBufferBuilder,
    pub features: FeatureMap,
    pub bounding_box: BoundingBox3,
    pub rotation_center: Vector3,
}

impl PartBuilder {
    pub fn new(
        part_builder: PartBufferBuilder,
        features: FeatureMap,
        bounding_box: BoundingBox3,
        rotation_center: &Vector3,
    ) -> Self {
        PartBuilder {
            part_builder,
            features,
            bounding_box,
            rotation_center: *rotation_center,
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

impl<'a> FaceVertices {
    pub fn center(&self) -> Vector3 {
        match self {
            FaceVertices::Triangle(a) => (a[0].position + a[1].position + a[2].position) / 3.0,
            FaceVertices::Quad(a) => (a[0].position + a[1].position + a[2].position + a[3].position) / 4.0,
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

    pub fn contains(&self, vec: &Vector3) -> bool {
        match self {
            FaceVertices::Triangle(v) => {
                for i in v {
                    if abs_diff_eq!(vec, &i.position) {
                        return true;
                    }
                }
            }
            FaceVertices::Quad(v) => {
                for i in v {
                    if abs_diff_eq!(vec, &i.position) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Face {
    vertices: FaceVertices,
    winding: Winding,
}

#[derive(Debug)]
struct Adjacency {
    pub position: Vector3,
    pub faces: Vec<(Rc<RefCell<Face>>, usize)>,
}

impl<'a> Adjacency {
    pub fn new(position: &Vector3) -> Adjacency {
        Adjacency {
            position: *position,
            faces: Vec::new(),
        }
    }

    pub fn add(&mut self, face: Rc<RefCell<Face>>, index: usize) {
        self.faces.push((Rc::clone(&face), index));
    }
}

fn calculate_normal(v1: &Vector3, v2: &Vector3, v3: &Vector3) -> Vector3 {
    (v2 - v3).cross(v2 - v1).normalize()
}

#[derive(Debug)]
struct MeshBuilder {
    pub faces: HashMap<MeshGroup, Vec<Rc<RefCell<Face>>>>,
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

    pub fn add(&mut self, group_key: &MeshGroup, face: Rc<RefCell<Face>>) {
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
                    let adjacency = Rc::new(RefCell::new(Adjacency::new(&vertex.position)));
                    adjacency.borrow_mut().add(Rc::clone(&face), index);
                    self.adjacencies.push(Rc::clone(&adjacency));
                    self.point_cloud.add(*vertex.position.as_ref(), adjacency).unwrap();
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
                    if flags[i] == false {
                        marked.clear();
                        marked.push(i);
                        flags[i] = true;
                        let (face, index) = &adjacency.faces[i];
                        let base_normal = face.borrow().vertices.query(*index).normal.clone();
                        let mut smoothed_normal = base_normal.clone();
                        for j in 0..length {
                            if i != j {
                                let (face, index) = &adjacency.faces[j];
                                let c_normal = face.borrow().vertices.query(*index).normal;
                                let angle = base_normal.angle(c_normal);
                                if angle.0 < f32::default_epsilon() {
                                    flags[j] = true;
                                }
                                if angle < NORMAL_BLEND_THRESHOLD {
                                    ops += 1;
                                    flags[j] = true;
                                    marked.push(j);
                                    smoothed_normal += c_normal;
                                }
                            }
                        }

                        if marked.len() > 0 {
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

    pub fn bake(&self, builder: &mut PartBufferBuilder, bounding_box: &mut BoundingBox3) {
        let mut bounding_box_min = None;
        let mut bounding_box_max = None;

        for (group_key, faces) in self.faces.iter() {
            let mesh = match builder.query_mesh(group_key) {
                Some(e) => e,
                None => {
                    println!("Skipping unknown color group_key {:?}", group_key);
                    continue;
                }
            };

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

                    mesh.add(&vertex.position, &vertex.normal);
                }
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
    enabled_features: Option<&'a HashSet<PartAlias>>,

    builder: PartBufferBuilder,
    mesh_builder: MeshBuilder,
    color_stack: Vec<ColorReference>,
    features: FeatureMap,
    bounding_box: BoundingBox3,
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

                    if self.enabled_features.is_some()
                        && self.enabled_features.unwrap().contains(&cmd.name)
                        && !invert_child
                    {
                        (*self
                            .features
                            .entry(cmd.name.clone())
                            .or_insert_with(Vec::new))
                        .push((color.clone(), matrix));
                    } else if let Some(part) = parent.get_subpart(&cmd.name) {
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

                    self.builder
                        .edges
                        .add(&(matrix * cmd.a).truncate(), &cmd.color, top);
                    self.builder
                        .edges
                        .add(&(matrix * cmd.b).truncate(), &cmd.color, top);
                }
                Command::OptionalLine(cmd) => {
                    let top = self.color_stack.last().unwrap();

                    self.builder.optional_edges.add(
                        &(matrix * cmd.a).truncate(),
                        &(matrix * cmd.b).truncate(),
                        &(matrix * cmd.c).truncate(),
                        &(matrix * cmd.d).truncate(),
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
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal: normal.clone(),
                                    },
                                ]),
                                winding: Winding::Ccw,
                            }
                        },
                        Winding::Cw => {
                            let v1 = (matrix * cmd.c).truncate();
                            let v2 = (matrix * cmd.b).truncate();
                            let v3 = (matrix * cmd.a).truncate();
                            let normal = calculate_normal(&v1, &v2, &v3);
                            Face {
                                vertices: FaceVertices::Triangle([
                                    FaceVertex {
                                        position: v1,
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal: normal.clone(),
                                    },
                                ]),
                                winding: Winding::Cw,
                            }
                        },
                    };

                    let category = MeshGroup {
                        color_ref: color.clone(),
                        bfc: if bfc_certified {
                            cull && local_cull
                        } else {
                            false
                        },
                    };

                    self.mesh_builder.add(&category, Rc::new(RefCell::new(face)));
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
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v4,
                                        normal: normal.clone(),
                                    },
                                ]),
                                winding: Winding::Ccw,
                            }
                        },
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
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v2,
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v3,
                                        normal: normal.clone(),
                                    },
                                    FaceVertex {
                                        position: v4,
                                        normal: normal.clone(),
                                    },
                                ]),
                                winding: Winding::Cw,
                            }
                        },
                    };

                    let category = MeshGroup {
                        color_ref: color.clone(),
                        bfc: if bfc_certified {
                            cull && local_cull
                        } else {
                            false
                        },
                    };

                    self.mesh_builder.add(&category, Rc::new(RefCell::new(face)));
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

    pub fn bake(&mut self) -> PartBuilder {
        let mut bounding_box = BoundingBox3::zero();
        self.mesh_builder.smooth_normals();
        self.mesh_builder.bake(&mut self.builder, &mut bounding_box);

        PartBuilder::new(
            mem::take(&mut self.builder),
            self.features.clone(),
            bounding_box,
            &Vector3::new(0.0, 0.0, 0.0),
        )
    }

    pub fn new(
        resolutions: &'a ResolutionResult,
        enabled_features: Option<&'a HashSet<PartAlias>>,
    ) -> Self {
        let mut mb = PartBaker {
            resolutions,
            enabled_features,

            builder: PartBufferBuilder::default(),
            mesh_builder: MeshBuilder::new(),
            color_stack: Vec::new(),
            features: HashMap::new(),
            bounding_box: BoundingBox3::zero(),
        };

        mb.color_stack.push(ColorReference::Current);

        mb
    }
}

pub fn bake_part<D: Deref<Target = MultipartDocument>>(
    resolutions: &ResolutionResult,
    enabled_features: Option<&HashSet<PartAlias>>,
    document: D,
    local: bool,
) -> PartBuilder {
    let mut baker = PartBaker::new(resolutions, enabled_features);

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
