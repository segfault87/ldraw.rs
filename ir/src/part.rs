use std::{
    collections::{HashMap, HashSet},
    f32, mem,
    ops::Deref,
    sync::Arc,
    vec::Vec,
};

use cgmath::{abs_diff_eq, AbsDiffEq, InnerSpace, Rad, SquareMatrix};
use kdtree::{distance::squared_euclidean, KdTree};
use ldraw::{
    color::{ColorReference, MaterialRegistry},
    document::Document,
    elements::{BfcStatement, Command, Meta},
    library::{ResolutionMap, ResolutionResult},
    AliasType, Matrix4, PartAlias, Vector3, Vector4, Winding,
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
                lhs[0].abs_diff_eq(&rhs[0], epsilon)
                    && lhs[1].abs_diff_eq(&rhs[1], epsilon)
                    && lhs[2].abs_diff_eq(&rhs[2], epsilon)
            }
            (FaceVertices::Quad(lhs), FaceVertices::Quad(rhs)) => {
                lhs[0].abs_diff_eq(&rhs[0], epsilon)
                    && lhs[1].abs_diff_eq(&rhs[1], epsilon)
                    && lhs[2].abs_diff_eq(&rhs[2], epsilon)
                    && lhs[3].abs_diff_eq(&rhs[3], epsilon)
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
            position: *position,
            faces: Vec::new(),
        }
    }

    pub fn add(&mut self, face: &Face) {
        self.faces.push(face.clone());
    }
}

#[derive(Debug)]
struct MeshBuilder {
    pub faces: HashMap<MeshGroup, Vec<Face>>,
    point_cloud: KdTree<f32, Adjacency, [f32; 3]>,
}

impl MeshBuilder {
    pub fn new() -> MeshBuilder {
        MeshBuilder {
            faces: HashMap::new(),
            point_cloud: KdTree::new(3),
        }
    }

    pub fn add(&mut self, group_key: &MeshGroup, face: Face) {
        let list = self.faces.entry(group_key.clone()).or_insert_with(Vec::new);
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
                    let mut adjacency = Adjacency::new(vertex);
                    adjacency.add(&face);
                    self.point_cloud.add(*vertex.as_ref(), adjacency).unwrap();
                }
            };
        }
    }

    pub fn bake(&self, builder: &mut PartBufferBuilder, bounding_box: &mut BoundingBox3) {
        let mut bounding_box_min = None;
        let mut bounding_box_max = None;

        for (group_key, faces) in self.faces.iter() {
            let mesh = builder.query_mesh(group_key);
            if mesh.is_none() {
                println!("Skipping unknown color group_key {:?}", group_key);
                continue;
            }
            let mesh = mesh.unwrap();

            for face in faces.iter() {
                let mut normal = face.vertices.normal();

                for vertex in face.vertices.triangles(false) {
                    match bounding_box_min {
                        None => {
                            bounding_box_min = Some(*vertex);
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
                            bounding_box_max = Some(*vertex);
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

                    let r: &[f32; 3] = vertex.as_ref();
                    if let Ok(mut matches) = self.point_cloud.iter_nearest(r, &squared_euclidean) {
                        if let Some(first_match) = matches.next() {
                            if first_match.0 < f32::default_epsilon() {
                                for face in first_match.1.faces.iter() {
                                    let fnormal = face.vertices.normal();
                                    if normal.angle(fnormal) < NORMAL_BLEND_THRESHOLD {
                                        normal = (normal + fnormal) * 0.5;
                                    }
                                }
                                normal = normal.normalize();
                            }
                        }
                    };

                    mesh.add(vertex, &normal);
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

struct PartBaker<'a, T> {
    resolutions: &'a ResolutionMap<'a, T>,
    enabled_features: Option<&'a HashSet<PartAlias>>,

    builder: PartBufferBuilder,
    mesh_builder: MeshBuilder,
    color_stack: Vec<ColorReference>,
    features: FeatureMap,
    bounding_box: BoundingBox3,
}

impl<'a, T: AliasType> PartBaker<'a, T> {
    pub fn traverse<D: Deref<Target = Document>>(
        &mut self,
        document: &D,
        matrix: Matrix4,
        cull: bool,
        invert: bool,
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
                    } else {
                        match self.resolutions.get(cmd) {
                            Some(ResolutionResult::Subpart(part)) => {
                                self.color_stack.push(color);
                                self.traverse(part, matrix, cull_next, invert_child);
                                self.color_stack.pop();
                            }
                            Some(ResolutionResult::Associated(part)) => {
                                self.color_stack.push(color);
                                self.traverse(&Arc::clone(part), matrix, cull_next, invert_child);
                                self.color_stack.pop();
                            }
                            _ => (),
                        }
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

                    let category = MeshGroup {
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

                    let category = MeshGroup {
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
            };
        }
    }

    pub fn bake(&mut self) -> PartBuilder {
        let mut bounding_box = BoundingBox3::zero();
        self.mesh_builder.bake(&mut self.builder, &mut bounding_box);

        PartBuilder::new(
            mem::take(&mut self.builder),
            self.features.clone(),
            bounding_box,
            &Vector3::new(0.0, 0.0, 0.0),
        )
    }

    pub fn new(
        resolutions: &'a ResolutionMap<T>,
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

pub fn bake_part<'a, T: AliasType>(
    resolution: &ResolutionMap<'a, T>,
    enabled_features: Option<&HashSet<PartAlias>>,
    document: &Document,
) -> PartBuilder {
    let mut baker = PartBaker::new(resolution, enabled_features);

    baker.traverse(&document, Matrix4::identity(), true, false);
    baker.bake()
}
