use std::collections::HashMap;

use cgmath::EuclideanSpace;
use glow::{Context as GlContext, HasContext};
use image::RgbaImage;
use ldraw::{
    color::Material,
    PartAlias, Point3,
};
use ldraw_renderer::{
    display_list::DisplayList,
    part::Part,
    state::{OrthographicCamera, OrthographicViewBounds},
};

use crate::{
    context::OlrContext,
    utils::calculate_bounding_box,
};

pub fn render_single_part(context: &OlrContext, part: &Part<GlContext>, material: &Material) -> RgbaImage {
    let gl = &context.gl;

    let mut rc = context.rendering_context.borrow_mut();

    unsafe {
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
    }

    let camera = OrthographicCamera::new_isometric(Point3::new(0.0, 0.0, 0.0));
    let bounds = rc.apply_orthographic_camera(&camera, &OrthographicViewBounds::BoundingBox3(part.bounding_box.clone())).unwrap();
    rc.render_single_part(&part, &material, false);
    rc.render_single_part(&part, &material, true);

    unsafe {
        gl.flush();
    }

    context.get_framebuffer_contents(Some(bounds))
}

pub fn render_display_list(
    context: &OlrContext,
    parts: &HashMap<PartAlias, Part<GlContext>>,
    display_list: &mut DisplayList<GlContext>
) -> RgbaImage {
    let gl = &context.gl;

    let mut rc = context.rendering_context.borrow_mut();

    unsafe {
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
    }

    let bounding_box = calculate_bounding_box(parts, display_list);
    let camera = OrthographicCamera::new_isometric(Point3::from_vec(bounding_box.center()));
    let bounds = rc.apply_orthographic_camera(&camera, &OrthographicViewBounds::BoundingBox3(bounding_box.clone())).unwrap();
    
    rc.render_display_list(&parts, display_list, false);
    rc.render_display_list(&parts, display_list, true);

    unsafe {
        gl.flush();
    }

    context.get_framebuffer_contents(Some(bounds))
}
