use std::{
    rc::Rc,
    vec::Vec,
};

use glow::{Context as GlContext, HasContext, PixelPackData};
use image::RgbaImage;
use ldraw::{
    color::Material,
    Point3
};
use ldraw_ir::{
    geometry::BoundingBox2,
    part::PartBuilder,
};
use ldraw_renderer::{
    part::Part,
    state::{OrthographicCamera, OrthographicViewBounds},
};

use crate::context::OlrContext;

fn buffer_to_image(context: &OlrContext, gl: Rc<GlContext>, bounds: &BoundingBox2) -> RgbaImage {
    let mut pixels: Vec<u8> = Vec::new();
    pixels.resize(4 * context.width * context.height, 0);
    unsafe {
        gl.read_buffer(glow::COLOR_ATTACHMENT0);
        gl.read_pixels(
            0, 0, context.width as _, context.height as _, glow::RGBA, glow::UNSIGNED_BYTE,
            PixelPackData::Slice(pixels.as_mut())
        );
    }

    let x1 = (bounds.min.x * context.width as f32) as usize;
    let y1 = (bounds.min.y * context.height as f32) as usize;
    let x2 = (bounds.max.x * context.width as f32) as usize;
    let y2 = (bounds.max.y * context.height as f32) as usize;
    let cw = x2 - x1;
    let ch = y2 - y1;

    let mut pixels_rearranged: Vec<u8> = Vec::new();
    for v in (y1..y2).rev() {
        let s = 4 * v as usize * context.width as usize;
        pixels_rearranged.extend_from_slice(&pixels[s..(s + (cw * 4))]);
    }

    RgbaImage::from_raw(cw as _, ch as _, pixels_rearranged).unwrap()
} 

pub fn render_single_part(context: &mut OlrContext, part: &PartBuilder, material: &Material) -> RgbaImage {
    let gl = &context.gl;

    let rc = &mut context.rendering_context;

    unsafe {
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
    }

    let part = Part::create(&part, Rc::clone(&gl));

    let camera = OrthographicCamera::new_isometric(Point3::new(0.0, 0.0, 0.0));
    let bounds = rc.apply_orthographic_camera(&camera, &OrthographicViewBounds::BoundingBox3(part.bounding_box.clone())).unwrap();
    rc.render_single_part(&part, &material, false);
    rc.render_single_part(&part, &material, true);

    unsafe {
        gl.flush();
    }

    buffer_to_image(context, Rc::clone(&gl), &bounds)
}
