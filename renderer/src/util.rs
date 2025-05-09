use std::hash::Hash;

use cgmath::SquareMatrix;
use ldraw::Matrix4;
use ldraw_ir::{
    geometry::BoundingBox3,
    model::{self, GroupId},
};

use crate::part::PartQuerier;

pub async fn request_device(
    adapter: &wgpu::Adapter,
    label: Option<&str>,
) -> Result<(wgpu::Device, wgpu::Queue, u32), wgpu::RequestDeviceError> {
    let texture_sizes = vec![8192, 4096, 2048];

    let mut error = None;

    for texture_size in texture_sizes {
        let required_limits = wgpu::Limits {
            max_texture_dimension_2d: texture_size,
            ..wgpu::Limits::downlevel_webgl2_defaults()
        };

        match adapter
            .request_device(&wgpu::DeviceDescriptor {
                label,
                required_features: wgpu::Features::default(),
                required_limits,
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await
        {
            Ok((device, queue)) => return Ok((device, queue, texture_size)),
            Err(e) => {
                error = Some(e);
            }
        }
    }

    Err(error.unwrap())
}

fn calculate_bounding_box_recursive<K: Clone + Eq + PartialEq + Hash, Q: PartQuerier<K>>(
    bb: &mut BoundingBox3,
    parts: &Q,
    matrix: Matrix4,
    items: &[model::Object<K>],
    model: &model::Model<K>,
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

pub fn calculate_model_bounding_box<K: Clone + Eq + PartialEq + Hash, Q: PartQuerier<K>>(
    model: &model::Model<K>,
    group_id: Option<GroupId>,
    parts: &Q,
) -> BoundingBox3 {
    let mut bb = BoundingBox3::nil();

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
