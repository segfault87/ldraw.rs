use std::{
    collections::HashMap,
    f32,
    rc::Rc,
    vec::Vec,
};

use cgmath::{Deg, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix};
use glow::HasContext;
use ldraw::{
    color::{ColorReference, Material, MaterialRegistry},
    elements::{Command, Meta},
    document::{Document, MultipartDocument},
    Matrix3, Matrix4, PartAlias, Vector3, Vector4,
};
use ldraw_renderer::{
    display_list::DisplayList,
    error::RendererError,
    part::Part,
    state::RenderingContext,
    shader::{ProgramManager},
};

#[derive(Clone, Debug)]
struct RenderingOrderItem {
    name: PartAlias,
    matrix: Matrix4,
    color: ColorReference,
}

#[derive(Debug)]
enum RenderingOrder {
    Item(RenderingOrderItem),
    Step,
}

fn traverse<'a, GL: HasContext>(
    gl: Rc<GL>,
    orders: &mut Vec<RenderingOrder>,
    document: &'a Document,
    matrix: Matrix4,
    parent: &'a MultipartDocument
) {
    for cmd in document.commands.iter() {
        match cmd {
            Command::Meta(meta) => {
                match meta {
                    Meta::Step => {
                        orders.push(RenderingOrder::Step);
                    },
                    _ => (),
                };
            },
            Command::PartReference(r) => {
                if parent.subparts.contains_key(&r.name) {
                    traverse(Rc::clone(&gl), orders, parent.subparts.get(&r.name).unwrap(), matrix * r.matrix, parent);
                } else {
                    orders.push(RenderingOrder::Item(RenderingOrderItem {
                        name: r.name.clone(),
                        matrix: matrix * r.matrix,
                        color: r.color.clone(),
                    }))
                }
            },
            _ => (),
        };
    }
}

fn create_rendering_list<GL: HasContext>(gl: Rc<GL>, document: &MultipartDocument) -> Vec<RenderingOrder> {
    let mut order = Vec::new();

    traverse(gl, &mut order, &document.body, Matrix4::identity(), document);

    order
}

pub struct App<GL: HasContext> {
    gl: Rc<GL>,

    features: HashMap<PartAlias, Part<GL>>,
    parts: HashMap<PartAlias, Part<GL>>,

    context: RenderingContext<GL>,
    display_list: DisplayList<GL>,
    rendering_order: Vec<RenderingOrder>,
    animating: Vec<(RenderingOrderItem, f32, Matrix4, f32, f32)>,
    playing: bool,
    pointer: Option<usize>,
    last_time: Option<f32>,
    frames: usize,
}

const FALL_INTERVAL: f32 = 0.2;
const FALL_DURATION: f32 = 0.5;

impl<GL: HasContext> App<GL> {

    pub fn new(
        gl: Rc<GL>,
        document: MultipartDocument,
        features: HashMap<PartAlias, Part<GL>>,
        parts: HashMap<PartAlias, Part<GL>>,
        program_manager: ProgramManager<GL>
    ) -> Self {
        let rendering_order = create_rendering_list(Rc::clone(&gl), &document);
        let context = RenderingContext::new(Rc::clone(&gl), program_manager);
        context.upload_shading_data();

        App {
            gl,
            features,
            parts,
            context,
            display_list: DisplayList::new(),
            rendering_order,
            animating: vec![],
            playing: true,
            pointer: None,
            last_time: None,
            frames: 0,
        }
    }

    pub fn set_up(&self) {
        self.context.set_initial_state();
    }

    pub fn advance(&mut self, time: f32) {
        let next = if self.pointer.is_none() && self.last_time.is_none()  {
            0
        } else if time - self.last_time.unwrap() >= FALL_INTERVAL {
            self.pointer.unwrap() + 1
        } else {
            return
        };

        if next >= self.rendering_order.len() {
            self.playing = false;
            return;
        }

        self.pointer = Some(next);
        match &self.rendering_order[next] {
            RenderingOrder::Item(item) => {
                println!("Add {:?}", item);
                self.animating.push((item.clone(), time, item.matrix.clone(), 0.0, 0.0));
                self.playing = true;
                self.last_time = Some(time);
            },
            RenderingOrder::Step => {
                self.playing = false;
            }
        };
    }

    pub fn animate(&mut self, time: f32) {
        self.context.camera.position.x = time.sin() * 500.0;
        self.context.camera.position.z = time.cos() * 500.0;
        self.context.update_camera();

        if self.playing {
            self.advance(time);
        }

        for (order, started_at, mat, opacity, progress) in self.animating.iter_mut() {
            let elapsed = (time - started_at.clone()).clamp(0.0, FALL_DURATION) / FALL_DURATION;
            let ease = -(f32::consts::FRAC_PI_2 + elapsed * f32::consts::FRAC_PI_2).cos();
            mat[3][1] = -(1.0 - ease) * 300.0 + order.matrix[3][1];
            *opacity = ease;
            *progress = elapsed;

            if progress >= &mut 1.0 {
                self.display_list.add(Rc::clone(&self.gl), order.name.clone(), order.matrix.clone(), order.color.clone());
            }
        }

        self.animating.retain(|(i, _, _, _, progress)| {
            if progress >= &1.0 {   
                println!("Remove one");
            }
            progress < &1.0
        });
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.context.resize(width, height);
    }

    pub fn render(&mut self) {
        let gl = &self.gl;

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            self.context.render_display_list(&self.parts, &mut self.display_list, false);
            self.context.render_display_list(&self.parts, &mut self.display_list, true);

            for (item, _, matrix, opacity, _) in self.animating.iter() {
                if let Some(part) = self.parts.get(&item.name) {
                    self.context.shading_data.opacity = opacity.clone();
                    self.context.projection_data.push_model_matrix(&matrix);
                    self.context.render_single_part(&part, &item.color, false);
                    self.context.render_single_part(&part, &item.color, true);
                    self.context.projection_data.pop_model_matrix();
                }
            }
            self.context.shading_data.opacity = 1.0;

            self.frames += 1;
            println!("Frames: {}", self.frames);

            gl.flush();
        }


    }

}
