use std::{
    collections::HashMap,
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

#[derive(Debug)]
enum RenderingOrder {
    Item {
        name: PartAlias,
        matrix: Matrix4,
        color: ColorReference,
    },
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
                    orders.push(RenderingOrder::Item {
                        name: r.name.clone(),
                        matrix: matrix * r.matrix,
                        color: r.color.clone(),
                    })
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
    playing: bool,
    pointer: Option<usize>,
    last_time: Option<f32>,
}

impl<GL: HasContext> App<GL> {

    pub fn new(
        gl: Rc<GL>,
        document: MultipartDocument,
        features: HashMap<PartAlias, Part<GL>>,
        parts: HashMap<PartAlias, Part<GL>>,
        program_manager: ProgramManager<GL>
    ) -> Self {
        let rendering_order = create_rendering_list(Rc::clone(&gl), &document);

        App {
            gl: Rc::clone(&gl),
            features,
            parts,
            context: RenderingContext::new(gl, program_manager),
            display_list: DisplayList::new(),
            rendering_order,
            playing: true,
            pointer: None,
            last_time: None,
        }
    }

    pub fn set_up(&self) {
        self.context.set_initial_state();
        self.context.upload_shading_data();
    }

    pub fn advance(&mut self, time: f32) {
        let next = if self.pointer.is_none() && self.last_time.is_none()  {
            0
        } else if time - self.last_time.unwrap() >= 0.1 {
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
            RenderingOrder::Item { name, matrix, color } => {
                self.display_list.add(Rc::clone(&self.gl), &name, &matrix, &color);
                self.playing = true;
                self.last_time = Some(time);
            },
            RenderingOrder::Step => {
                self.playing = false;
                return;
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
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.context.resize(width, height);
    }

    pub fn render(&mut self) {
        let gl = &self.gl;

        self.context.start_render();

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            self.context.render_display_list(&self.parts, &mut self.display_list);
            gl.flush();
        }
    }

}
