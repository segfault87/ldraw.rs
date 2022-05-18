use std::{
    collections::{HashMap, HashSet},
    f32,
    rc::Rc,
    sync::{Arc, RwLock},
    vec::Vec,
};

use cgmath::{Deg, SquareMatrix};
use glow::HasContext;
use ldraw::{
    color::{ColorReference, Material, MaterialRegistry},
    document::{Document, MultipartDocument},
    elements::{Command, Meta},
    error::ResolutionError,
    library::{resolve_dependencies_multipart, LibraryLoader, PartCache},
    Matrix4, PartAlias, Point2, Point3, Vector2, Vector3,
};
use ldraw_ir::{
    geometry::BoundingBox3,
    model::Model,
    part::bake_multipart_document
};
use ldraw_renderer::{
    display_list::DisplayList,
    model::RenderableModel,
    part::{Part, PartsPool},
    shader::ProgramManager,
    state::{PerspectiveCamera, RenderingContext},
};
use uuid::Uuid;

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

    tick: Option<f32>,
    velocity: Vector2,

    pub camera: PerspectiveCamera,
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

            velocity: Vector2::new(0.1, 0.0),
            tick: None,

            camera: PerspectiveCamera::new(
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(0.0, 0.0, 0.0),
                Deg(45.0),
            ),
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
                self.longitude = (self.longitude + (y - last_pos.y) * 0.01).clamp(
                    -f32::consts::FRAC_PI_2 + 0.017,
                    f32::consts::FRAC_PI_2 - 0.017,
                );
            }
            self.last_pos = Some(Point2::new(x, y));
        }
    }

    pub fn update(&mut self, tick: f32) {
        if let Some(t) = self.tick {
            let delta = tick - t;

            self.latitude += self.velocity.x * delta;
            self.longitude = (self.longitude + self.velocity.y * delta).clamp(
                -f32::consts::FRAC_PI_2 + 0.017,
                f32::consts::FRAC_PI_2 - 0.017,
            );
        }

        self.tick = Some(tick);

        self.camera.position = self.derive_coordinate();
    }

    fn derive_coordinate(&self) -> Point3 {
        let look_at = &self.camera.look_at;
        let x = self.latitude.sin() * self.longitude.cos() * self.radius + look_at.x;
        let y = -self.longitude.sin() * self.radius + look_at.y;
        let z = -self.latitude.cos() * self.longitude.cos() * self.radius + look_at.z;

        Point3::new(x, y, z)
    }
}

impl Default for OrbitController {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum State {
    Playing,
    Step,
    Finished,
}


struct SimplePartsPool<GL: HasContext>(pub HashMap<PartAlias, Arc<Part<GL>>>);

impl<GL: HasContext> PartsPool<GL> for SimplePartsPool<GL> {

    fn query(&self, alias: &PartAlias) -> Option<Arc<Part<GL>>> {
        self.0.get(alias).map(Arc::clone)
    }

}

impl<GL: HasContext> SimplePartsPool<GL> {
    pub fn new() -> Self {
        SimplePartsPool(HashMap::new())
    }
}

pub struct App<GL: HasContext> {
    gl: Rc<GL>,

    loader: Rc<Box<dyn LibraryLoader>>,
    materials: Rc<MaterialRegistry>,

    parts: Arc<RwLock<SimplePartsPool<GL>>>,

    context: RenderingContext<GL>,
    model: Option<RenderableModel<GL, SimplePartsPool<GL>>>,

    pub orbit: OrbitController,

    pub state: State,
    frames: usize,
}

/*const FALL_INTERVAL: f32 = 0.2;
const FALL_INTERVAL_UPPER_BOUND: f32 = 5.0;
const FALL_DURATION: f32 = 0.5;*/

impl<GL: HasContext> App<GL>
{
    pub fn new(
        gl: Rc<GL>,
        loader: Rc<Box<dyn LibraryLoader>>,
        materials: Rc<MaterialRegistry>,
        program_manager: ProgramManager<GL>,
    ) -> Self {
        let context = RenderingContext::new(Rc::clone(&gl), program_manager);
        context.upload_shading_data();

        App {
            gl: Rc::clone(&gl),
            loader,
            materials,
            parts: Arc::new(RwLock::new(SimplePartsPool::new())),
            context,
            model: None,
            orbit: OrbitController::default(),
            state: State::Finished,
            frames: 0,
        }
    }

    pub fn loaded_parts(&self) -> HashSet<PartAlias> {
        let mut result = HashSet::new();

        result.extend(self.parts.read().unwrap().0.keys().cloned());

        result
    }

    pub async fn set_document<F: Fn(PartAlias, Result<(), ResolutionError>)>(
        &mut self,
        cache: Arc<RwLock<PartCache>>,
        document: &MultipartDocument,
        on_update: &F,
    ) -> Result<(), ResolutionError> {
        let resolution_result = resolve_dependencies_multipart(
            Arc::clone(&cache),
            &self.materials,
            &self.loader,
            document,
            on_update,
        )
        .await;

        let model = Model::from_ldraw_multipart_document(document, &self.materials, Some((&self.loader, cache))).await;

        self.parts.write().unwrap().0.extend(
            document
                .list_dependencies()
                .into_iter()
                .filter_map(|alias| {
                    resolution_result.query(&alias, true).map(|(part, local)| {
                        (
                            alias.clone(),
                            Arc::new(Part::create(
                                &bake_multipart_document(&resolution_result, None, part, local),
                                Rc::clone(&self.gl),
                                &self.materials
                            )),
                        )
                    })
                })
        );
        self.state = State::Playing;
        let model = RenderableModel::new(model, Rc::clone(&self.gl), Arc::clone(&self.parts), &self.materials);
        let bounding_box = model.bounding_box.clone();
        let center = bounding_box.center();
        self.model = Some(model);
        self.orbit.camera.look_at = Point3::new(center.x, center.y, center.z);
        self.orbit.radius = (bounding_box.len_x() * bounding_box.len_x()
            + bounding_box.len_y() * bounding_box.len_y()
            + bounding_box.len_z() * bounding_box.len_z())
        .sqrt()
            * 2.0;

        Ok(())
    }

    pub fn set_up(&self) {
        self.context.set_initial_state();
    }

    pub fn advance(&mut self, time: f32) {
        /*if self.state == State::Step || self.pointer.is_none() {
            let start = self.pointer.unwrap_or(0);

            let mut count = 0;
            for i in start..self.rendering_order.len() {
                if let RenderingOrder::Step = self.rendering_order[i] {
                    break;
                }
                count += 1;
            }

            self.fall_interval = if count as f32 * FALL_INTERVAL >= FALL_INTERVAL_UPPER_BOUND {
                FALL_INTERVAL_UPPER_BOUND / count as f32
            } else {
                FALL_INTERVAL
            };
        }

        let next = if self.pointer.is_none() && self.last_time.is_none() {
            0
        } else if time - self.last_time.unwrap() >= self.fall_interval {
            self.pointer.unwrap() + 1
        } else {
            return;
        };

        if next >= self.rendering_order.len() {
            self.state = State::Finished;
            return;
        }

        self.pointer = Some(next);
        match &self.rendering_order[next] {
            RenderingOrder::Item(item) => {
                self.animating
                    .push((item.clone(), time, item.matrix, 0.0, 0.0));
                self.state = State::Playing;
                self.last_time = Some(time);
            }
            RenderingOrder::Step => {
                self.state = State::Step;
            }
        };*/
    }

    pub fn animate(&mut self, time: f32) {
        self.orbit.update(time);

        self.context.apply_perspective_camera(&self.orbit.camera);

        if self.state == State::Playing {
            self.advance(time);
        }

        /*
        for (order, started_at, mat, opacity, progress) in self.animating.iter_mut() {
            if *progress < 1.0 {
                let elapsed = (time - *started_at).clamp(0.0, FALL_DURATION) / FALL_DURATION;
                let ease = -(f32::consts::FRAC_PI_2 + elapsed * f32::consts::FRAC_PI_2).cos();
                mat[3][1] = -(1.0 - ease) * 300.0 + order.matrix[3][1];
                *opacity = ease;
                *progress = elapsed;

                if *progress >= 1.0 {
                    let mut tr = self.display_list.start_modification();
                    tr.add(
                        Rc::clone(&self.gl),
                        order.name.clone(),
                        order.matrix,
                        order.material.clone(),
                    );
                    tr.end();
                }
            }
        }

        self.animating
            .retain(|(_, _, _, _, progress)| *progress < 1.0);
        */
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.context.resize(width, height);
    }

    pub fn render(&mut self) {
        let gl = &self.gl;

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }

        if let Some(ref model) = self.model {
            model.render(&mut self.context, false);
            model.render(&mut self.context, true);
        }

        self.frames += 1;

        unsafe {
            gl.flush();
        }
    }

    pub fn get_subparts(&self) -> Vec<(Uuid, String)> {
        if let Some(v) = &self.model {
            let mut result = v.model.object_groups.iter().map(
                |(k, v)| (k.clone(), v.name.clone())
            ).collect::<Vec<_>>();
            result.sort_by(|a, b| a.1.cmp(&b.1));
            result
        } else {
            Vec::new()
        }
    }

    pub fn set_render_target(&mut self, group_id: Option<Uuid>) {
        if let Some(v) = &mut self.model {
            v.set_render_target(group_id);

            let bounding_box = match group_id {
                None => v.bounding_box.clone(),
                Some(uuid) => if let Some(v) = v.subpart_bounding_boxes.get(&uuid) {
                    v.clone()
                } else {
                    BoundingBox3::zero()
                }
            };
            let center = bounding_box.center();
            self.orbit.camera.look_at = Point3::new(center.x, center.y, center.z);
            self.orbit.radius = (bounding_box.len_x() * bounding_box.len_x()
                + bounding_box.len_y() * bounding_box.len_y()
                + bounding_box.len_z() * bounding_box.len_z())
            .sqrt()
                * 2.0;
        }
    }
}
