use std::collections::HashMap;

use glow::{Context as GlContext, HasContext};
use ldraw::PartAlias;
use ldraw_ir::geometry::BoundingBox3;
use ldraw_renderer::{
    display_list::DisplayList,
    part::Part,
};

pub fn calculate_bounding_box(
    parts: &HashMap<PartAlias, Part<GlContext>>,
    display_list: &DisplayList<GlContext>,
) -> BoundingBox3 {
    let mut bb = BoundingBox3::zero();

    for (key, value) in display_list.map.iter() {
        if let Some(part) = parts.get(key) {
            if let Some(ibb) = value.opaque.calculate_bounding_box(&part.bounding_box) {
                bb.update(&ibb);
            }
            if let Some(ibb) = value.translucent.calculate_bounding_box(&part.bounding_box) {
                bb.update(&ibb);
            }
        }
    }

    bb
}
