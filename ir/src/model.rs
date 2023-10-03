use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::{Arc, RwLock},
    vec::Vec,
};

use cgmath::SquareMatrix;
use ldraw::{
    color::{ColorCatalog, ColorReference},
    document::{Document as LdrawDocument, MultipartDocument as LdrawMultipartDocument},
    elements::{Command, Meta},
    library::{resolve_dependencies, LibraryLoader, PartCache, ResolutionResult},
    Matrix4, PartAlias, Vector3,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    geometry::BoundingBox3,
    part::{
        bake_part_from_document, bake_part_from_multipart_document, Part, PartDimensionQuerier,
    },
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ObjectInstance<P> {
    Part(PartInstance<P>),
    PartGroup(PartGroupInstance),
    Step,
    Annotation(Annotation),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Object<P> {
    pub id: Uuid,
    pub data: ObjectInstance<P>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PartInstance<P> {
    pub matrix: Matrix4,
    pub color: ColorReference,
    pub part: P,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PartGroupInstance {
    pub matrix: Matrix4,
    pub color: ColorReference,
    pub group_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Annotation {
    pub position: Vector3,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ObjectGroup<P> {
    pub id: Uuid,
    pub name: String,
    pub objects: Vec<Object<P>>,
    pub pivot: Vector3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model<P: Clone + Eq + PartialEq + Hash> {
    pub object_groups: HashMap<Uuid, ObjectGroup<P>>,
    pub objects: Vec<Object<P>>,
    pub embedded_parts: HashMap<P, Part>,
}

impl<P: Clone + Eq + PartialEq + Hash> Default for Model<P> {
    fn default() -> Self {
        Self {
            object_groups: HashMap::new(),
            objects: Vec::new(),
            embedded_parts: HashMap::new(),
        }
    }
}

fn build_objects<P: Clone + Eq + PartialEq + Hash + From<PartAlias>>(
    document: &LdrawDocument,
    subparts: Option<&HashMap<P, Uuid>>,
) -> Vec<Object<P>> {
    document
        .commands
        .iter()
        .filter_map(|cmd| {
            let data = match cmd {
                Command::PartReference(r) => match subparts {
                    Some(subparts) => match subparts.get(&r.name.clone().into()) {
                        Some(e) => Some(ObjectInstance::PartGroup(PartGroupInstance {
                            matrix: r.matrix,
                            color: r.color.clone(),
                            group_id: *e,
                        })),
                        None => Some(ObjectInstance::Part(PartInstance {
                            matrix: r.matrix,
                            color: r.color.clone(),
                            part: r.name.clone().into(),
                        })),
                    },
                    None => Some(ObjectInstance::Part(PartInstance {
                        matrix: r.matrix,
                        color: r.color.clone(),
                        part: r.name.clone().into(),
                    })),
                },
                Command::Meta(Meta::Step) => Some(ObjectInstance::Step),
                _ => None,
            };

            data.map(|v| Object {
                id: Uuid::new_v4(),
                data: v,
            })
        })
        .collect::<Vec<_>>()
}

fn resolve_colors<P>(objects: &mut [Object<P>], colors: &ColorCatalog) {
    for object in objects.iter_mut() {
        match &mut object.data {
            ObjectInstance::Part(ref mut p) => {
                p.color.resolve_self(colors);
            }
            ObjectInstance::PartGroup(ref mut pg) => {
                pg.color.resolve_self(colors);
            }
            _ => {}
        }
    }
}

fn extract_document_primitives<P: From<PartAlias>>(
    document: &LdrawDocument,
) -> Option<(P, Part, Object<P>)> {
    if document.has_primitives() {
        let name = &document.name;
        let prims = LdrawDocument {
            name: name.clone(),
            description: document.description.clone(),
            author: document.author.clone(),
            bfc: document.bfc.clone(),
            headers: document.headers.clone(),
            commands: document
                .commands
                .iter()
                .filter_map(|e| match e {
                    Command::Line(_)
                    | Command::Triangle(_)
                    | Command::Quad(_)
                    | Command::OptionalLine(_) => Some(e.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
        };
        let prims = LdrawMultipartDocument {
            body: prims,
            subparts: HashMap::new(),
        };

        let part = bake_part_from_multipart_document(&prims, &ResolutionResult::default(), true);
        let object = Object {
            id: Uuid::new_v4(),
            data: ObjectInstance::Part(PartInstance {
                matrix: Matrix4::identity(),
                color: ColorReference::Current,
                part: PartAlias::from(name.clone()).into(),
            }),
        };

        Some((PartAlias::from(name.clone()).into(), part, object))
    } else {
        None
    }
}

impl<P: Eq + PartialEq + Hash + Clone + From<PartAlias>> Model<P> {
    pub fn from_ldraw_multipart_document_sync(document: &LdrawMultipartDocument) -> Self {
        let subparts = document
            .subparts
            .keys()
            .map(|alias| (alias.clone().into(), Uuid::new_v4()))
            .collect::<HashMap<_, _>>();

        let mut embedded_parts: HashMap<P, Part> = HashMap::new();

        let mut object_groups = HashMap::new();
        for (alias, subpart) in document.subparts.iter() {
            let converted_alias = alias.clone().into();
            if !embedded_parts.contains_key(&converted_alias) {
                let id = *subparts.get(&converted_alias).unwrap();
                object_groups.insert(
                    id,
                    ObjectGroup {
                        id,
                        name: subpart.name.clone(),
                        objects: build_objects::<P>(subpart, Some(&subparts)),
                        pivot: Vector3::new(0.0, 0.0, 0.0),
                    },
                );
            }
        }
        let mut objects = build_objects::<P>(&document.body, Some(&subparts));

        if let Some((alias, part, object)) = extract_document_primitives::<P>(&document.body) {
            embedded_parts.insert(alias.clone(), part);
            objects.push(object);
        }

        Model {
            object_groups,
            objects,
            embedded_parts: HashMap::new(),
        }
    }

    pub async fn from_ldraw_multipart_document<L: LibraryLoader>(
        document: &LdrawMultipartDocument,
        colors: &ColorCatalog,
        inline_loader: Option<(&L, Arc<RwLock<PartCache>>)>,
    ) -> Self {
        let subparts = document
            .subparts
            .keys()
            .map(|alias| (alias.clone().into(), Uuid::new_v4()))
            .collect::<HashMap<_, _>>();

        let mut embedded_parts: HashMap<P, Part> = HashMap::new();
        if let Some((loader, cache)) = inline_loader {
            for (alias, subpart) in document.subparts.iter() {
                if subpart.has_primitives() {
                    let resolution_result = resolve_dependencies(
                        subpart,
                        Arc::clone(&cache),
                        colors,
                        loader,
                        &|_, _| {},
                    )
                    .await;

                    let part = bake_part_from_document(subpart, &resolution_result, true);

                    embedded_parts.insert(alias.clone().into(), part);
                }
            }
        }

        let mut object_groups = HashMap::new();
        for (alias, subpart) in document.subparts.iter() {
            let converted_alias = alias.clone().into();
            if !embedded_parts.contains_key(&converted_alias) {
                let id = *subparts.get(&converted_alias).unwrap();
                object_groups.insert(
                    id,
                    ObjectGroup {
                        id,
                        name: subpart.name.clone(),
                        objects: build_objects::<P>(subpart, Some(&subparts)),
                        pivot: Vector3::new(0.0, 0.0, 0.0),
                    },
                );
            }
        }
        let mut objects = build_objects::<P>(&document.body, Some(&subparts));

        if let Some((alias, part, object)) = extract_document_primitives::<P>(&document.body) {
            embedded_parts.insert(alias.clone(), part);
            objects.push(object);
        }

        Model {
            object_groups,
            objects,
            embedded_parts,
        }
    }

    pub fn from_ldraw_document(document: &LdrawDocument) -> Self {
        let mut embedded_parts = HashMap::new();
        let mut objects = build_objects(document, None);

        if let Some((alias, part, object)) = extract_document_primitives::<P>(document) {
            embedded_parts.insert(alias.clone(), part);
            objects.push(object);
        }

        Model {
            object_groups: HashMap::new(),
            objects,
            embedded_parts,
        }
    }

    pub fn resolve_colors(&mut self, colors: &ColorCatalog) {
        resolve_colors(&mut self.objects, colors);
        for group in self.object_groups.values_mut() {
            resolve_colors(&mut group.objects, colors);
        }
    }

    fn build_dependencies(&self, deps: &mut HashSet<P>, objects: &[Object<P>]) {
        for object in objects.iter() {
            match &object.data {
                ObjectInstance::Part(p) => {
                    if !deps.contains(&p.part) {
                        deps.insert(p.part.clone());
                    }
                }
                ObjectInstance::PartGroup(pg) => {
                    if let Some(pg) = self.object_groups.get(&pg.group_id) {
                        self.build_dependencies(deps, &pg.objects);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn list_dependencies(&self) -> HashSet<P> {
        let mut dependencies = HashSet::new();

        self.build_dependencies(&mut dependencies, &self.objects);

        dependencies
    }

    fn calculate_bounding_box_recursive(
        &self,
        bounding_box: &mut BoundingBox3,
        matrix: Matrix4,
        objects: &[Object<P>],
        mut complete: bool,
        querier: &impl PartDimensionQuerier<P>,
    ) -> bool {
        for item in objects.iter() {
            match &item.data {
                ObjectInstance::Part(part) => {
                    if let Some(dim) = querier.query_part_dimension(&part.part) {
                        bounding_box.update(&dim.transform(&matrix));
                    } else {
                        complete = false;
                    }
                }
                ObjectInstance::PartGroup(pg) => {
                    if let Some(group) = self.object_groups.get(&pg.group_id) {
                        self.calculate_bounding_box_recursive(
                            bounding_box,
                            matrix * pg.matrix,
                            &group.objects,
                            complete,
                            querier,
                        );
                    }
                }
                _ => {}
            }
        }

        complete
    }

    pub fn calculate_bounding_box(
        &self,
        group_id: Option<Uuid>,
        querier: &impl PartDimensionQuerier<P>,
    ) -> Option<(BoundingBox3, bool)> {
        let objects = if let Some(group_id) = group_id {
            &self.object_groups.get(&group_id)?.objects
        } else {
            &self.objects
        };

        let matrix = Matrix4::identity();
        let mut bounding_box = BoundingBox3::nil();

        let complete = self.calculate_bounding_box_recursive(
            &mut bounding_box,
            matrix,
            &objects,
            true,
            querier,
        );

        Some((bounding_box, complete))
    }
}
