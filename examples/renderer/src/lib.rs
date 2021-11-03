use std::{
    collections::{HashSet, HashMap},
    f32,
    rc::Rc,
    vec::Vec,
};

use async_trait::async_trait;
use cgmath::{Deg, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix};
use glow::HasContext;
use ldraw::{
    color::{ColorReference, Material, MaterialRegistry},
    elements::{Command, Meta},
    document::{Document, MultipartDocument},
    Matrix3, Matrix4, PartAlias, Vector3, Vector4,
};
use ldraw_ir::part::PartBuilder;
use ldraw_renderer::{
    display_list::DisplayList,
    error::RendererError,
    part::Part,
    state::RenderingContext,
    shader::{ProgramManager},
};

#[async_trait]
pub trait ResourceLoader {
    async fn load(
        &mut self, locator: &String, loaded: &HashSet<&PartAlias>
    ) -> Result<(MultipartDocument, HashMap<PartAlias, PartBuilder>, HashMap<PartAlias, PartBuilder>), &'static str>;
}

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

#[derive(Eq, PartialEq)]
pub enum State {
    Playing,
    Step,
    Finished,
}

pub struct App<GL: HasContext> {
    gl: Rc<GL>,

    features: HashMap<PartAlias, Part<GL>>,
    parts: HashMap<PartAlias, Part<GL>>,

    context: RenderingContext<GL>,
    display_list: DisplayList<GL>,
    rendering_order: Vec<RenderingOrder>,
    animating: Vec<(RenderingOrderItem, f32, Matrix4, f32, f32)>,

    state: State,
    pointer: Option<usize>,
    last_time: Option<f32>,
    frames: usize,
}

const FALL_INTERVAL: f32 = 0.2;
const FALL_DURATION: f32 = 0.5;

impl<GL: HasContext> App<GL> {

    pub fn new(
        gl: Rc<GL>,
        program_manager: ProgramManager<GL>
    ) -> Self {
        let context = RenderingContext::new(Rc::clone(&gl), program_manager);
        context.upload_shading_data();

        App {
            gl,
            features: HashMap::new(),
            parts: HashMap::new(),
            context,
            display_list: DisplayList::new(),
            rendering_order: Vec::new(),
            animating: Vec::new(),
            state: State::Finished,
            pointer: None,
            last_time: None,
            frames: 0,
        }
    }

    pub async fn set_document<RL: ResourceLoader>(&mut self, loader: &mut RL, locator: &String) -> Result<(), &'static str> {
        let mut cache = HashSet::new();
        cache.extend(self.features.keys());
        cache.extend(self.parts.keys());

        let (document, features, parts) = loader.load(locator, &cache).await?;

        let features = features.iter().map(|(k, v)| (k.clone(), Part::create(&v, Rc::clone(&self.gl)))).collect::<HashMap<_, _>>();
        let parts = parts.iter().map(|(k, v)| (k.clone(), Part::create(&v, Rc::clone(&self.gl)))).collect::<HashMap<_, _>>();

        self.features.extend(features);
        self.parts.extend(parts);
        self.state = State::Playing;
        self.animating = Vec::new();
        self.display_list = DisplayList::new();
        self.pointer = None;
        self.last_time = None;
        self.rendering_order = create_rendering_list(Rc::clone(&self.gl), &document);

        Ok(())
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
            self.state = State::Finished;
            return;
        }

        self.pointer = Some(next);
        match &self.rendering_order[next] {
            RenderingOrder::Item(item) => {
                self.animating.push((item.clone(), time, item.matrix.clone(), 0.0, 0.0));
                self.state = State::Playing;
                self.last_time = Some(time);
            },
            RenderingOrder::Step => {
                self.state = State::Step;
            }
        };
    }

    pub fn animate(&mut self, time: f32) {
        self.context.camera.position.x = time.sin() * 500.0;
        self.context.camera.position.z = time.cos() * 500.0;
        self.context.update_camera();

        if self.state == State::Playing {
            self.advance(time);
        }

        let mut clear = true;
        for (order, started_at, mat, opacity, progress) in self.animating.iter_mut() {
            if *progress < 1.0 {
                let elapsed = (time - started_at.clone()).clamp(0.0, FALL_DURATION) / FALL_DURATION;
                let ease = -(f32::consts::FRAC_PI_2 + elapsed * f32::consts::FRAC_PI_2).cos();
                mat[3][1] = -(1.0 - ease) * 300.0 + order.matrix[3][1];
                *opacity = ease;
                *progress = elapsed;

                clear = false;
            }
        }

        /*if clear {
            for (order, _, _, _, _) in self.animating.iter() {
                self.display_list.add(Rc::clone(&self.gl), order.name.clone(), order.matrix.clone(), order.color.clone());
            }
            self.animating.clear();
        }*/
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.context.resize(width, height);
    }

    pub fn render(&mut self, max: Option<usize>) {
        let gl = &self.gl;

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }

        //self.context.render_display_list(&self.parts, &mut self.display_list, false);
        //self.context.render_display_list(&self.parts, &mut self.display_list, true);

        for (i, (item, _, matrix, opacity, _)) in self.animating.iter().enumerate() {
            if let Some(part) = self.parts.get(&item.name) {
                self.context.shading_data.opacity = opacity.clone();
                self.context.projection_data.push_model_matrix(&matrix);
                self.context.render_single_part(&part, &item.color, false);
                self.context.render_single_part(&part, &item.color, true);
                self.context.projection_data.pop_model_matrix();
            }

            if let Some(max) = max {
                if max == i {
                    break;
                }
            }
        }
        self.context.shading_data.opacity = 1.0;

        self.frames += 1;

        unsafe {
            gl.flush();
        }


    }

}
