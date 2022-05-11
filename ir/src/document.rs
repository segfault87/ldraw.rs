use std::{
    collections::HashMap,
    fmt::{Formatter, Result as FmtResult},
    vec::Vec,
};

use ldraw::{
    color::{ColorReference, MaterialRegistry},
    document::{
        Document as LdrawDocument,
        MultipartDocument as LdrawMultipartDocument
    },
    Matrix4, PartAlias, Vector3,
};
use serde::{
    de::{self, Deserialize as DeserializeT, Deserializer, Visitor, MapAccess},
    ser::{self, Serialize as SerializeT, SerializeMap, Serializer},
    Deserialize, Serialize
};
use uuid::Uuid;

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
pub struct Document {
    pub object_groups: HashMap<Uuid, ObjectGroup>,
    pub objects: Vec<Object>,
}

impl SerializeT for Document {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("object_groups", &self.object_groups.values().collect::<Vec<_>>())?;
        map.serialize_entry("objects", &self.objects)?;
        map.end()
    }
}

impl<'de> DeserializeT<'de> for Document {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            ObjectGroups,
            Objects,
        }

        struct DocumentVisitor;

        impl<'de> Visitor<'de> for DocumentVisitor {
            type Value = Document;

            fn expecting(&self, formatter: &mut Formatter) -> FmtResult {
                formatter.write_str("struct Formatter")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Document, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut object_groups = None;
                let mut objects = None;

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
                    }
                }

                let object_groups: Vec<ObjectGroup> = object_groups.ok_or_else(
                    || de::Error::missing_field("object_groups")
                )?;
                let object_groups = object_groups.into_iter().map(|v| (v.id.clone(), v)).collect::<HashMap<_, _>>();
                let objects = objects.ok_or_else(|| de::Error::missing_field("objects"))?;

                Ok(Document { object_groups, objects })
            }
        }

        const FIELDS: &'static [&'static str] = &["object_groups", "items"];


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

impl Document {

    pub fn from_ldraw_multipart_document(document: &LdrawMultipartDocument) -> Self {
        let subparts = document.subparts.keys().map(|alias| (alias.clone(), Uuid::new_v4())).collect::<HashMap<_, _>>();

        Document {
            object_groups: document.subparts.iter().map(|(alias, subpart)| {
                let id = subparts.get(&alias).unwrap().clone();
                (
                    id.clone(),
                    ObjectGroup {
                        id,
                        name: subpart.name.clone(),
                        objects: build_objects(subpart, Some(&subparts)),
                        pivot: Vector3::new(0.0, 0.0, 0.0),
                    }
                )
            }).collect::<HashMap<_, _>>(),
            objects: build_objects(&document.body, Some(&subparts)),
        }
    }

    pub fn from_ldraw_document(document: &LdrawDocument) -> Self {
        Document {
            object_groups: HashMap::new(),
            objects: build_objects(&document, None),
        }
    }

    pub fn resolve_colors(&mut self, materials: &MaterialRegistry) {
        resolve_colors(&mut self.objects, materials);
        for group in self.object_groups.values_mut() {
            resolve_colors(&mut group.objects, materials);
        }
    }

}
