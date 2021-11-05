use std::{
    collections::{HashSet, HashMap},
    f32,
    future::Future,
    rc::Rc,
    vec::Vec,
};

use cgmath::{SquareMatrix, Zero};
use glow::HasContext;
use ldraw::{
    color::{ColorReference, Material, MaterialRegistry},
    elements::{Command, Meta},
    document::{Document, MultipartDocument},
    Matrix3, Matrix4, PartAlias, Point2, Point3, Vector2, Vector3, Vector4,
};
use ldraw_ir::{
    part::PartBuilder,
    BoundingBox
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

pub struct OrbitController {
    last_pos: Option<Point2>,
    pressed_at: f32,
    pressed_position: Point2,
    released_at: f32,
    released_position: Point2,
    pressing: bool,

    latitude: f32,
    longitude: f32,

    pub radius: f32,
    pub center: Point3,

    tick: Option<f32>,
    velocity: Vector2,
}

impl OrbitController {

    pub fn new() -> Self {
        OrbitController {
            last_pos: None,
            pressed_at: 0.0,
            pressed_position: Point2::new(0.0, 0.0),
            released_at: 0.0,
            released_position: Point2::new(0.0, 0.0),
            pressing: false,

            latitude: 0.785,
            longitude: 0.262,
            radius: 300.0,
            center: Point3::new(0.0, 0.0, 0.0),

            velocity: Vector2::new(0.1, 0.0),
            tick: None,
        }
    }

    pub fn on_mouse_press(&mut self, pressed: bool) {
        self.pressing = pressed;

        if !pressed {
            self.last_pos = None;
        }
    }

    pub fn on_mouse_move(&mut self, x: f32, y: f32) {
        if self.pressing {
            if let Some(last_pos) = self.last_pos {
                self.latitude -= (x - last_pos.x) * 0.01;
                self.longitude = (self.longitude + (y - last_pos.y) * 0.01).clamp(-f32::consts::FRAC_PI_2 + 0.017, f32::consts::FRAC_PI_2 - 0.017);
            }
            self.last_pos = Some(Point2::new(x, y));
        }
    }

    pub fn update(&mut self, tick: f32) {
        if let Some(t) = self.tick {
            let delta = tick - t;

            self.latitude += self.velocity.x * delta;
            self.longitude = (self.longitude + self.velocity.y * delta).clamp(-f32::consts::FRAC_PI_2 + 0.017, f32::consts::FRAC_PI_2 - 0.017);
        }

        self.tick = Some(tick);
    }

    pub fn derive_coordinate(&self) -> Point3 {
        let x = self.latitude.sin() * self.longitude.cos() * self.radius + self.center.x;
        let y = -self.longitude.sin() * self.radius + self.center.y;
        let z = -self.latitude.cos() * self.longitude.cos() * self.radius + self.center.z;

        Point3::new(x, y, z)
    }
}

#[derive(Debug)]
enum RenderingOrder {
    Item(RenderingOrderItem),
    Step,
}

fn traverse<'a, GL: HasContext>(
    gl: Rc<GL>,
    orders: &mut Vec<RenderingOrder>,
    bb: &mut BoundingBox,
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
                    traverse(Rc::clone(&gl), orders, bb, parent.subparts.get(&r.name).unwrap(), matrix * r.matrix, parent);
                } else {
                    let matrix = matrix * r.matrix;
                    // FIXME: unsimplify
                    let center = Vector3::new(matrix[3][0], matrix[3][1], matrix[3][2]);
                    bb.update_point(&center);
                    orders.push(RenderingOrder::Item(RenderingOrderItem {
                        name: r.name.clone(),
                        matrix,
                        color: r.color.clone(),
                    }))
                }
            },
            _ => (),
        };
    }
}

fn create_rendering_list<GL: HasContext>(gl: Rc<GL>, document: &MultipartDocument) -> (Vec<RenderingOrder>, BoundingBox) {
    let mut order = Vec::new();
    let mut bb = BoundingBox::zero();

    traverse(gl, &mut order, &mut bb, &document.body, Matrix4::identity(), document);

    (order, bb)
}

#[derive(Copy, Clone, Eq, PartialEq)]
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
    pub orbit: OrbitController,

    pub state: State,
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
            display_list: DisplayList::default(),
            rendering_order: Vec::new(),
            animating: Vec::new(),
            orbit: OrbitController::new(),
            state: State::Finished,
            pointer: None,
            last_time: None,
            frames: 0,
        }
    }

    pub fn part_count(&self) -> usize {
        let mut len = 0;

        for i in self.rendering_order.iter() {
            if let RenderingOrder::Item(_) = i {
                len += 1;
            }
        }

        len
    }

    pub fn loaded_parts(&self) -> HashSet<&PartAlias> {
        let mut result = HashSet::new();

        result.extend(self.features.keys());
        result.extend(self.parts.keys());

        result
    }

    pub fn set_document(
        &mut self,
        document: &MultipartDocument,
        features: &HashMap<PartAlias, PartBuilder>,
        parts: &HashMap<PartAlias, PartBuilder>,
    ) {
        let mut cache = HashSet::new();
        cache.extend(self.features.keys());
        cache.extend(self.parts.keys());

        let features = features.iter().map(|(k, v)| (k.clone(), Part::create(&v, Rc::clone(&self.gl)))).collect::<HashMap<_, _>>();
        let parts = parts.iter().map(|(k, v)| (k.clone(), Part::create(&v, Rc::clone(&self.gl)))).collect::<HashMap<_, _>>();

        self.features.extend(features);
        self.parts.extend(parts);
        self.state = State::Playing;
        self.animating = Vec::new();
        self.display_list = DisplayList::default();
        self.pointer = None;
        self.last_time = None;
        let (rendering_order, bounding_box) = create_rendering_list(Rc::clone(&self.gl), &document);
        self.rendering_order = rendering_order;
        let center = bounding_box.center();
        self.orbit.center = Point3::new(center.x, center.y, center.z);
        self.orbit.radius = (
            bounding_box.len_x() * bounding_box.len_x() +
            bounding_box.len_y() * bounding_box.len_y() +
            bounding_box.len_z() * bounding_box.len_z()
        ).sqrt() * 2.0;
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
        self.orbit.update(time);

        self.context.camera.position = self.orbit.derive_coordinate();
        self.context.camera.look_at = self.orbit.center;
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
