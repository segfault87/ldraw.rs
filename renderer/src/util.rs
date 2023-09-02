use cgmath::SquareMatrix;
use ldraw::{Matrix4, PartAlias};
use ldraw_ir::{geometry::BoundingBox3, model};
use uuid::Uuid;

use crate::part::PartQuerier;

fn calculate_bounding_box_recursive(
    bb: &mut BoundingBox3,
    parts: &impl PartQuerier<PartAlias>,
    matrix: Matrix4,
    items: &[model::Object],
    model: &model::Model,
) {
    for item in items.iter() {
        match &item.data {
            model::ObjectInstance::Part(p) => {
                if let Some(embedded_part) = model.embedded_parts.get(&p.part) {
                    bb.update(&embedded_part.bounding_box.transform(&(matrix * p.matrix)));
                } else if let Some(part) = parts.get(&p.part) {
                    bb.update(&part.bounding_box.transform(&(matrix * p.matrix)));
                }
            }
            model::ObjectInstance::PartGroup(pg) => {
                if let Some(group) = model.object_groups.get(&pg.group_id) {
                    calculate_bounding_box_recursive(
                        bb,
                        parts,
                        matrix * pg.matrix,
                        &group.objects,
                        model,
                    );
                }
            }
            _ => {}
        }
    }
}

pub fn calculate_model_bounding_box(
    model: &model::Model,
    group_id: Option<Uuid>,
    parts: &impl PartQuerier<PartAlias>,
) -> BoundingBox3 {
    let mut bb = BoundingBox3::zero();

    if let Some(group_id) = group_id {
        if let Some(subpart) = model.object_groups.get(&group_id) {
            calculate_bounding_box_recursive(
                &mut bb,
                parts,
                Matrix4::identity(),
                &subpart.objects,
                model,
            );
        }
    } else {
        calculate_bounding_box_recursive(
            &mut bb,
            parts,
            Matrix4::identity(),
            &model.objects,
            model,
        );
    }

    bb
}
