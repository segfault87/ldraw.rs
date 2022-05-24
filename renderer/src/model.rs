use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{Arc, RwLock},
};

use glow::HasContext;
use ldraw::{color::ColorCatalog, PartAlias};
use ldraw_ir::{
    geometry::BoundingBox3,
    model::{Model, Object, ObjectInstance},
};
use uuid::Uuid;

use crate::{
    display_list::DisplayList,
    part::{Part, PartsPool},
    state::RenderingContext,
};

pub struct RenderableModel<GL: HasContext, P: PartsPool<GL>> {
    parts: Arc<RwLock<P>>,

    pub model: Model,
    pub display_list: DisplayList<GL>,
    pub embedded_parts: HashMap<PartAlias, Part<GL>>,

    pub bounding_box: BoundingBox3,
    pub subpart_bounding_boxes: HashMap<Uuid, BoundingBox3>,
    pub display_target: Option<Uuid>,
    pub exclusion_set: HashSet<Uuid>,
}

fn calculate_bounding_box<GL: HasContext, P: PartsPool<GL>>(
    objects: &[Object],
    model: &Model,
    parts: Arc<RwLock<P>>,
    subpart_bounding_boxes: &mut HashMap<Uuid, BoundingBox3>,
) -> BoundingBox3 {
    let mut bb = BoundingBox3::zero();

    for object in objects.iter() {
        let (matrix, bounding_box) = match &object.data {
            ObjectInstance::Part(part_instance) => {
                let matrix = part_instance.matrix;

                let bounding_box =
                    if let Some(part) = parts.read().unwrap().query(&part_instance.part) {
                        part.bounding_box.clone()
                    } else {
                        continue;
                    };

                (matrix, bounding_box)
            }
            ObjectInstance::PartGroup(group_instance) => {
                let matrix = group_instance.matrix;

                let bounding_box = match subpart_bounding_boxes.get(&group_instance.group_id) {
                    Some(sub_bb) => sub_bb.clone(),
                    None => {
                        if let Some(group) = model.object_groups.get(&group_instance.group_id) {
                            let sub_bb = calculate_bounding_box(
                                &group.objects,
                                model,
                                Arc::clone(&parts),
                                subpart_bounding_boxes,
                            );
                            subpart_bounding_boxes.insert(group_instance.group_id, sub_bb.clone());

                            sub_bb
                        } else {
                            continue;
                        }
                    }
                };

                (matrix, bounding_box)
            }
            _ => continue,
        };

        bb.update(&bounding_box.translate(&matrix));
    }

    bb
}

fn calculate_subpart_bounding_boxes<GL: HasContext, P: PartsPool<GL>>(
    model: &Model,
    parts: Arc<RwLock<P>>,
    subpart_bounding_boxes: &mut HashMap<Uuid, BoundingBox3>,
) {
    for (id, subpart) in model.object_groups.iter() {
        if !subpart_bounding_boxes.contains_key(id) {
            let bb = calculate_bounding_box(
                &subpart.objects,
                model,
                Arc::clone(&parts),
                subpart_bounding_boxes,
            );
            subpart_bounding_boxes.insert(*id, bb);
        }
    }
}

impl<GL: HasContext, P: PartsPool<GL>> RenderableModel<GL, P> {
    pub fn new(
        model: Model,
        gl: Rc<GL>,
        parts_pool: Arc<RwLock<P>>,
        colors: &ColorCatalog,
    ) -> Self {
        let display_list = DisplayList::from_model(&model, Rc::clone(&gl));
        let embedded_parts = model
            .embedded_parts
            .iter()
            .map(|(alias, part)| (alias.clone(), Part::create(part, Rc::clone(&gl), colors)))
            .collect::<HashMap<_, _>>();

        let mut subpart_bounding_boxes = HashMap::new();
        let bounding_box = calculate_bounding_box(
            &model.objects,
            &model,
            Arc::clone(&parts_pool),
            &mut subpart_bounding_boxes,
        );
        calculate_subpart_bounding_boxes(
            &model,
            Arc::clone(&parts_pool),
            &mut subpart_bounding_boxes,
        );

        RenderableModel {
            parts: parts_pool,

            model,
            embedded_parts,
            display_list,

            bounding_box,
            subpart_bounding_boxes,
            display_target: None,
            exclusion_set: HashSet::new(),
        }
    }

    fn update_display_list(&mut self) {
        self.display_list
            .rebuild(&self.model, self.display_target, &self.exclusion_set);
    }

    pub fn set_render_target(&mut self, group_id: Option<Uuid>) {
        self.display_target = group_id;
        self.update_display_list();
    }

    pub fn clear_exclusion_set(&mut self) {
        if !self.exclusion_set.is_empty() {
            self.exclusion_set.clear();
            self.update_display_list();
        }
    }

    pub fn hide(&mut self, object_id: Uuid) {
        self.exclusion_set.insert(object_id);
        self.update_display_list();
    }

    pub fn render(&self, context: &mut RenderingContext<GL>, translucent: bool) {
        if let Ok(parts) = self.parts.read() {
            let display_items = if translucent {
                &self.display_list.translucent
            } else {
                &self.display_list.opaque
            };
            for (alias, object) in display_items.iter() {
                match self.embedded_parts.get(alias) {
                    Some(e) => context.render_instanced(e, object, translucent),
                    None => match parts.query(alias) {
                        Some(e) => context.render_instanced(&e, object, translucent),
                        None => continue,
                    },
                }
            }
        }
    }
}
