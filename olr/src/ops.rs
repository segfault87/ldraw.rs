use std::{
    rc::Rc,
    sync::{Arc, RwLock},
};

use cgmath::EuclideanSpace;
use glow::{Context as GlContext, HasContext};
use image::RgbaImage;
use ldraw::{color::{Material, MaterialRegistry}, Point3};
use ldraw_ir::model::Model;
use ldraw_renderer::{
    model::RenderableModel,
    part::{Part, PartsPool},
    state::{OrthographicCamera, OrthographicViewBounds},
};

use crate::context::OlrContext;

pub fn render_single_part(
    part: &Part<GlContext>,
    context: &OlrContext,
    material: &Material,
) -> RgbaImage {
    let gl = &context.gl;

    let mut rc = context.rendering_context.borrow_mut();

    unsafe {
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
    }

    let camera = OrthographicCamera::new_isometric(Point3::new(0.0, 0.0, 0.0));
    let bounds = rc
        .apply_orthographic_camera(
            &camera,
            &OrthographicViewBounds::BoundingBox3(part.bounding_box.clone()),
        )
        .unwrap();
    rc.render_single_part(part, material, false);
    rc.render_single_part(part, material, true);

    unsafe {
        gl.flush();
    }

    context.get_framebuffer_contents(Some(bounds))
}

pub fn render_model<P: PartsPool<GlContext>>(
    model: &Model,
    context: &OlrContext,
    parts: Arc<RwLock<P>>,
    colors: &MaterialRegistry,
) -> RgbaImage {
    let gl = &context.gl;

    let mut rc = context.rendering_context.borrow_mut();

    let renderable_model = RenderableModel::new(model.clone(), Rc::clone(&gl), parts, colors);

    unsafe {
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
    }

    let bounding_box = &renderable_model.bounding_box;
    let camera = OrthographicCamera::new_isometric(Point3::from_vec(bounding_box.center()));
    let bounds = rc
        .apply_orthographic_camera(&camera, &OrthographicViewBounds::BoundingBox3(bounding_box.clone()))
        .unwrap();

    renderable_model.render(&mut rc, false);
    renderable_model.render(&mut rc, true);

    unsafe {
        gl.flush();
    }

    context.get_framebuffer_contents(Some(bounds))
}
