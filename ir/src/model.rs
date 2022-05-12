use std::{
    collections::HashMap,
    fmt::{Formatter, Result as FmtResult},
    vec::Vec,
};

use cgmath::SquareMatrix;
use ldraw::{
    color::{ColorReference, MaterialRegistry},
    document::{
        Document as LdrawDocument,
        MultipartDocument as LdrawMultipartDocument
    },
    elements::Command,
    library::ResolutionResult,
    Matrix4, PartAlias, Vector3,
};
use serde::{
    de::{self, Deserialize as DeserializeT, Deserializer, Visitor, MapAccess},
    ser::{self, Serialize as SerializeT, SerializeMap, Serializer},
    Deserialize, Serialize
};
use uuid::Uuid;

use crate::part::{PartBuilder, bake_part};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ObjectInstance {
    Part(PartInstance),
    PartGroup(PartGroupInstance),
    Step,
    Annotation(Annotation),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Object {
    pub id: Uuid,
    pub data: ObjectInstance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PartInstance {
    pub matrix: Matrix4,
    pub color: ColorReference,
    pub part: PartAlias,
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
pub struct ObjectGroup {
    pub id: Uuid,
    pub name: String,
    pub objects: Vec<Object>,
    pub pivot: Vector3,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub object_groups: HashMap<Uuid, ObjectGroup>,
    pub objects: Vec<Object>,
    pub embedded_parts: HashMap<PartAlias, PartBuilder>,
}

impl SerializeT for Model {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("object_groups", &self.object_groups.values().collect::<Vec<_>>())?;
        map.serialize_entry("objects", &self.objects)?;
        map.serialize_entry("embedded_parts", &self.embedded_parts)?;
        map.end()
    }
}

impl<'de> DeserializeT<'de> for Model {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            ObjectGroups,
            Objects,
            EmbeddedParts,
        }

        struct DocumentVisitor;

        impl<'de> Visitor<'de> for DocumentVisitor {
            type Value = Model;

            fn expecting(&self, formatter: &mut Formatter) -> FmtResult {
                formatter.write_str("struct Formatter")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Model, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut object_groups = None;
                let mut objects = None;
                let mut embedded_parts = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::ObjectGroups => {
                            if object_groups.is_some() {
                                return Err(de::Error::duplicate_field("object_groups"));
                            }
                            object_groups = Some(map.next_value()?);
                        }
                        Field::Objects => {
                            if objects.is_some() {
                                return Err(de::Error::duplicate_field("objects"));
                            }
                            objects = Some(map.next_value()?);
                        }
                        Field::EmbeddedParts => {
                            if embedded_parts.is_some() {
                                return Err(de::Error::duplicate_field("embedded_parts"));
                            }
                            embedded_parts = Some(map.next_value()?);
                        }
                    }
                }

                let object_groups: Vec<ObjectGroup> = object_groups.unwrap_or_else(Vec::new);
                let object_groups = object_groups.into_iter().map(|v| (v.id.clone(), v)).collect::<HashMap<_, _>>();
                let objects = objects.ok_or_else(|| de::Error::missing_field("objects"))?;
                let embedded_parts = embedded_parts.unwrap_or_else(HashMap::new);

                Ok(Model { object_groups, objects, embedded_parts })
            }
        }

        const FIELDS: &'static [&'static str] = &["object_groups", "items", "embedded_parts"];


        deserializer.deserialize_struct("Document", FIELDS, DocumentVisitor)
    }
}

fn build_objects(document: &LdrawDocument, subparts: Option<&HashMap<PartAlias, Uuid>>) -> Vec<Object> {
    document.iter_refs().map(|v| {
        let data = match subparts {
            Some(subparts) => {
                match subparts.get(&v.name) {
                    Some(e) => ObjectInstance::PartGroup(
                        PartGroupInstance {
                            matrix: v.matrix.clone(),
                            color: v.color.clone(),
                            group_id: e.clone(),
                        }
                    ),
                    None => ObjectInstance::Part(
                        PartInstance {
                            matrix: v.matrix.clone(),
                            color: v.color.clone(),
                            part: v.name.clone(),
                        }
                    )
                }
            },
            None => {
                ObjectInstance::Part(
                    PartInstance {
                        matrix: v.matrix.clone(),
                            color: v.color.clone(),
                            part: v.name.clone(),
                    }
                )
            }
        };

        Object {
            id: Uuid::new_v4(),
            data,
        }
    }).collect::<Vec<_>>()
}

fn resolve_colors(objects: &mut Vec<Object>, materials: &MaterialRegistry) {
    for object in objects.iter_mut() {
        match &mut object.data {
            ObjectInstance::Part(ref mut p) => {
                p.color.resolve_self(materials);
            }
            ObjectInstance::PartGroup(ref mut pg) => {
                pg.color.resolve_self(materials);
            }
            _ => {}
        }
    }
}

fn extract_document_primitives(document: &LdrawDocument) -> Option<(PartAlias, PartBuilder, Object)> {
    if document.has_primitives() {
        let name = &document.name;
        let prims = LdrawDocument {
            name: name.clone(),
            description: document.description.clone(),
            author: document.author.clone(),
            bfc: document.bfc.clone(),
            headers: document.headers.clone(),
            commands: document.commands.iter().filter_map(|e| {
                match e {
                    Command::Line(_) | Command::Triangle(_) | Command::Quad(_) | Command::OptionalLine(_) => Some(e.clone()),
                    _ => None
                }
            }).collect::<Vec<_>>(),
        };
        let prims = LdrawMultipartDocument {
            body: prims,
            subparts: HashMap::new(),
        };

        let part = bake_part(&ResolutionResult::default(), None, &prims, true);
        let object = Object {
            id: Uuid::new_v4(),
            data: ObjectInstance::Part(
                PartInstance {
                    matrix: Matrix4::identity(),
                    color: ColorReference::Current,
                    part: PartAlias::from(name.clone()),
                }
            ),
        };

        Some((PartAlias::from(name.clone()), part, object))
    } else {
        None
    }
}

impl Model {

    pub async fn from_ldraw_multipart_document(document: &LdrawMultipartDocument, ) -> Self {
        let subparts = document.subparts.keys().map(|alias| (alias.clone(), Uuid::new_v4())).collect::<HashMap<_, _>>();

        let object_groups = document.subparts.iter().filter_map(|(alias, subpart)| {
            let id = subparts.get(&alias).unwrap().clone();

            Some((
                id.clone(),
                ObjectGroup {
                    id,
                    name: subpart.name.clone(),
                    objects: build_objects(subpart, Some(&subparts)),
                    pivot: Vector3::new(0.0, 0.0, 0.0),
                }
            ))
        }).collect::<HashMap<_, _>>();

        let mut embedded_parts = HashMap::new();
        let mut objects = build_objects(&document.body, Some(&subparts));
        
        if let Some((alias, part, object)) = extract_document_primitives(&document.body) {
            embedded_parts.insert(alias, part);
            objects.push(object);
        }

        Model { object_groups, objects, embedded_parts }
    }

    pub async fn from_ldraw_document(document: &LdrawDocument) -> Self {
        let mut embedded_parts = HashMap::new();
        let mut objects = build_objects(&document, None);

        if let Some((alias, part, object)) = extract_document_primitives(&document) {
            embedded_parts.insert(alias, part);
            objects.push(object);
        }

        Model {
            object_groups: HashMap::new(),
            objects,
            embedded_parts,
        }
    }

    pub fn resolve_colors(&mut self, materials: &MaterialRegistry) {
        resolve_colors(&mut self.objects, materials);
        for group in self.object_groups.values_mut() {
            resolve_colors(&mut group.objects, materials);
        }
    }

}