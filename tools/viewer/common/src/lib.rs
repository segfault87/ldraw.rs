mod texture;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    f32,
    rc::Rc,
    sync::{Arc, RwLock},
    vec::Vec,
};

use cgmath::{Deg, SquareMatrix};
use instant::{Duration, Instant};
use ldraw::{
    color::{Color, ColorCatalog},
    document::MultipartDocument,
    error::ResolutionError,
    library::{resolve_dependencies_multipart, LibraryLoader, PartCache},
    Matrix4, PartAlias, Point2, Point3, Vector2,
};
use ldraw_ir::{model, part::bake_part_from_multipart_document};
use ldraw_renderer::{
    camera::{PerspectiveCamera, Projection},
    display_list::DisplayList,
    part::{Part, PartQuerier},
    pipeline::RenderingPipelineManager,
    util::calculate_model_bounding_box,
};
use uuid::Uuid;
use winit::{event, window::Window};

use self::texture::Texture;

pub struct OrbitController {
    last_pos: Option<Point2>,
    pressing: bool,

    latitude: f32,
    longitude: f32,

    pub radius: f32,

    tick: Option<f32>,
    velocity: Vector2,

    camera: PerspectiveCamera,
}

impl OrbitController {
    pub fn new() -> Self {
        let camera = PerspectiveCamera::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            Deg(45.0),
        );

        OrbitController {
            last_pos: None,
            pressing: false,

            latitude: 0.785,
            longitude: 0.262,

            radius: 300.0,

            velocity: Vector2::new(0.1, 0.0),
            tick: None,

            camera,
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

    pub fn zoom(&mut self, delta: f32) {
        if self.radius - delta > 0.0 {
            self.radius -= delta;
        }
    }

    pub fn update(
        &mut self,
        projection: &mut Projection,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        tick: Option<f32>,
    ) {
        if let (Some(p), Some(n)) = (self.tick, tick) {
            let delta = n - p;

            self.latitude += self.velocity.x * delta;
            self.longitude = (self.longitude + self.velocity.y * delta).clamp(
                -f32::consts::FRAC_PI_2 + 0.017,
                f32::consts::FRAC_PI_2 - 0.017,
            );
        }
        self.tick = tick;

        self.camera.position = self.derive_coordinate();
        projection.update_camera(queue, &self.camera, (width, height).into());
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

#[derive(Default)]
struct SimplePartsPool(pub HashMap<PartAlias, Part>);

impl PartQuerier<PartAlias> for SimplePartsPool {
    fn get(&self, alias: &PartAlias) -> Option<&Part> {
        self.0.get(alias)
    }
}

const FALL_INTERVAL: f32 = 0.2;
const FALL_INTERVAL_UPPER_BOUND: f32 = 10.0;
const FALL_DURATION: f32 = 0.5;

#[derive(Clone, Debug)]
struct RenderingItem {
    id: uuid::Uuid,
    alias: PartAlias,
    matrix: Matrix4,
    color: Color,
}

enum RenderingStep {
    Item(RenderingItem),
    Step,
}

#[derive(Debug)]
struct AnimatingRenderingItem {
    item: RenderingItem,
    started_at: f32,
    progress: f32,
}

struct AnimatedModel {
    display_list: DisplayList<uuid::Uuid, PartAlias>,
    items: Vec<RenderingStep>,
    animating: RefCell<Vec<AnimatingRenderingItem>>,

    state: State,
    pointer: Option<usize>,
    fall_interval: f32,
    last_time: Option<f32>,
}

impl Default for AnimatedModel {
    fn default() -> Self {
        Self {
            display_list: DisplayList::new(),
            items: Vec::new(),
            animating: RefCell::new(Vec::new()),

            state: State::Finished,
            pointer: None,
            fall_interval: FALL_INTERVAL,
            last_time: None,
        }
    }
}

impl AnimatedModel {
    fn uuid_xor(a: Uuid, b: Uuid) -> Uuid {
        let ba = a.to_bytes_le();
        let bb = b.to_bytes_le();

        let bc: Vec<_> = ba.iter().zip(bb).map(|(x, y)| x ^ y).collect();
        Uuid::from_slice(&bc).unwrap()
    }

    fn build_item_recursive(
        items: &mut Vec<RenderingStep>,
        model: &model::Model,
        objects: &[model::Object],
        parent_uuid: uuid::Uuid,
        matrix: Matrix4,
    ) {
        for object in objects {
            match &object.data {
                model::ObjectInstance::Step => items.push(RenderingStep::Step),
                model::ObjectInstance::Part(p) => items.push(RenderingStep::Item(RenderingItem {
                    id: Self::uuid_xor(parent_uuid, object.id),
                    alias: p.part.clone(),
                    matrix: matrix * p.matrix,
                    color: p.color.get_color().cloned().unwrap_or_default(),
                })),
                model::ObjectInstance::PartGroup(pg) => {
                    if let Some(group) = model.object_groups.get(&pg.group_id) {
                        Self::build_item_recursive(
                            items,
                            model,
                            &group.objects,
                            object.id,
                            matrix * pg.matrix,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub fn from_model(
        model: &model::Model,
        group_id: Option<Uuid>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_catalog: &ColorCatalog,
        animated: bool,
    ) -> Self {
        if animated {
            let objects = if let Some(group_id) = group_id {
                model
                    .object_groups
                    .get(&group_id)
                    .map(|group| &group.objects)
            } else {
                Some(&model.objects)
            };

            let mut items = Vec::new();
            if let Some(objects) = objects {
                Self::build_item_recursive(
                    &mut items,
                    model,
                    objects,
                    uuid::Uuid::nil(),
                    Matrix4::identity(),
                );
            }

            let items_len = items.len();

            Self {
                display_list: DisplayList::new(),
                items,
                animating: RefCell::new(Vec::new()),

                state: State::Playing,
                pointer: None,
                fall_interval: if items_len as f32 * FALL_INTERVAL >= FALL_INTERVAL_UPPER_BOUND {
                    FALL_INTERVAL_UPPER_BOUND / items_len as f32
                } else {
                    FALL_INTERVAL
                },
                last_time: None,
            }
        } else {
            let display_list =
                DisplayList::from_model(model, group_id, device, queue, color_catalog);

            Self {
                display_list,
                items: Vec::new(),
                animating: RefCell::new(Vec::new()),

                state: State::Finished,
                pointer: None,
                fall_interval: 0.0,
                last_time: None,
            }
        }
    }

    pub fn advance(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, time: f32) {
        if self.state == State::Step || self.pointer.is_none() {
            let start = self.pointer.unwrap_or(0);

            let mut count = 0;
            for i in start..self.items.len() {
                if let RenderingStep::Step = self.items[i] {
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

        if next >= self.items.len() {
            self.state = State::Finished;
            return;
        }

        self.pointer = Some(next);
        match &self.items[next] {
            RenderingStep::Item(item) => {
                self.animating.borrow_mut().push(AnimatingRenderingItem {
                    item: item.clone(),
                    started_at: time,
                    progress: 0.0,
                });
                self.display_list.modify(device, queue, |tr| {
                    tr.insert(
                        item.alias.clone(),
                        item.id,
                        item.matrix,
                        &item.color,
                        Some(0.0),
                    );
                    true
                });
                self.state = State::Playing;
                self.last_time = Some(time);
            }
            RenderingStep::Step => {
                self.state = State::Step;
            }
        }
    }

    pub fn animate(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, time: f32) {
        if self.state == State::Playing {
            self.advance(device, queue, time);
        }

        let mut animating = self.animating.borrow_mut();

        animating.retain(|v| time - v.started_at < FALL_DURATION * 2.0);

        self.display_list.modify(device, queue, move |tr| {
            let mut modified = false;

            for item in animating.iter_mut() {
                modified = true;

                let elapsed = (time - item.started_at).clamp(0.0, FALL_DURATION) / FALL_DURATION;

                let ease = -(f32::consts::FRAC_PI_2 + elapsed * f32::consts::FRAC_PI_2).cos();
                let alpha = ease * (item.item.color.color.alpha() as f32 / 255.0);

                let mut matrix = item.item.matrix;
                matrix[3][1] = item.item.matrix[3][1] + (-(1.0 - ease) * 300.0);
                tr.update_matrix(item.item.id, matrix);
                tr.update_alpha(item.item.id, alpha);

                item.progress = elapsed;
            }

            modified
        });
    }
}

pub struct App {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    window: Window,

    framebuffer_texture: Option<Texture>,
    depth_texture: Texture,
    sample_count: u32,

    projection: Projection,
    pipelines: RenderingPipelineManager,

    loader: Rc<dyn LibraryLoader>,
    colors: Rc<ColorCatalog>,

    parts: Arc<RwLock<SimplePartsPool>>,
    model: Option<model::Model>,
    animated_model: AnimatedModel,

    orbit_controller: RefCell<OrbitController>,
}

impl App {
    pub async fn new(
        window: Window,
        loader: Rc<dyn LibraryLoader>,
        colors: Rc<ColorCatalog>,
        supports_line_rendering: bool,
        supports_antialiasing: bool,
    ) -> Self {
        let size = window.inner_size();

        let backends = wgpu::util::backend_bits_from_env().unwrap_or_else(wgpu::Backends::all);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            dx12_shader_compiler: Default::default(),
        });

        let sample_count = if supports_antialiasing { 4 } else { 1 };

        let surface = match unsafe { instance.create_surface(&window) } {
            Ok(surface) => surface,
            Err(e) => panic!("cannot create surface: {}", e),
        };

        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, Some(&surface))
            .await
            .unwrap();

        let line_features = if supports_line_rendering {
            wgpu::Features::POLYGON_MODE_LINE
        } else {
            wgpu::Features::empty()
        };

        let limits = wgpu::Limits {
            max_texture_dimension_2d: 8192,
            ..wgpu::Limits::downlevel_webgl2_defaults()
        };

        let (device, queue) = match adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::default() | line_features,
                    limits,
                },
                None,
            )
            .await
        {
            Ok(v) => v,
            Err(e) => panic!("Could not create device: {}", e),
        };

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![surface_format],
        };
        surface.configure(&device, &config);

        let framebuffer_texture = if sample_count > 1 {
            Some(Texture::create_framebuffer(
                &device,
                &config,
                sample_count,
                Some("Multisample framebuffer"),
            ))
        } else {
            None
        };
        let depth_texture =
            Texture::create_depth_texture(&device, &config, sample_count, Some("Depth texture"));

        let mut projection = Projection::new(&device);
        let orbit_controller = RefCell::new(OrbitController::default());
        orbit_controller.borrow_mut().update(
            &mut projection,
            &queue,
            size.width,
            size.height,
            None,
        );

        let pipelines = RenderingPipelineManager::new(
            &device,
            &queue,
            config.format,
            supports_line_rendering,
            sample_count,
        );

        App {
            surface,
            device,
            queue,
            config,
            size,
            window,

            framebuffer_texture,
            depth_texture,
            sample_count,

            projection,
            pipelines,

            loader,
            colors,

            parts: Arc::new(RwLock::new(SimplePartsPool::default())),
            model: None,
            animated_model: AnimatedModel::default(),

            orbit_controller,
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
            document,
            Arc::clone(&cache),
            &self.colors,
            &*self.loader,
            on_update,
        )
        .await;

        let model = model::Model::from_ldraw_multipart_document(
            document,
            &self.colors,
            Some((&*self.loader, cache)),
        )
        .await;

        self.parts
            .write()
            .unwrap()
            .0
            .extend(
                document
                    .list_dependencies()
                    .into_iter()
                    .filter_map(|alias| {
                        resolution_result.query(&alias, true).map(|(part, local)| {
                            (
                                alias.clone(),
                                Part::new(
                                    &bake_part_from_multipart_document(
                                        part,
                                        &resolution_result,
                                        local,
                                    ),
                                    &self.device,
                                    &self.colors,
                                    self.pipelines.supports_line_rendering(),
                                ),
                            )
                        })
                    }),
            );

        let bounding_box = calculate_model_bounding_box(&model, None, &*self.parts.read().unwrap());
        let center = bounding_box.center();

        self.animated_model =
            AnimatedModel::from_model(&model, None, &self.device, &self.queue, &self.colors, true);
        self.model = Some(model);

        let mut orbit_controller = self.orbit_controller.borrow_mut();
        orbit_controller.camera.look_at = Point3::new(center.x, center.y, center.z);
        orbit_controller.radius = (bounding_box.len_x() * bounding_box.len_x()
            + bounding_box.len_y() * bounding_box.len_y()
            + bounding_box.len_z() * bounding_box.len_z())
        .sqrt()
            * 2.0;

        Ok(())
    }

    pub fn advance(&mut self, time: f32) {
        self.animated_model.advance(&self.device, &self.queue, time);
    }

    pub fn animate(&mut self, time: f32) {
        self.orbit_controller.borrow_mut().update(
            &mut self.projection,
            &self.queue,
            self.size.width,
            self.size.height,
            Some(time),
        );

        self.animated_model.animate(&self.device, &self.queue, time);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            self.orbit_controller.borrow_mut().update(
                &mut self.projection,
                &self.queue,
                self.size.width,
                self.size.height,
                None,
            );

            self.framebuffer_texture = if self.sample_count > 1 {
                Some(Texture::create_framebuffer(
                    &self.device,
                    &self.config,
                    self.sample_count,
                    Some("Multisample framebuffer"),
                ))
            } else {
                None
            };
            self.depth_texture = Texture::create_depth_texture(
                &self.device,
                &self.config,
                self.sample_count,
                Some("Depth texture"),
            );
        }
    }

    pub fn render(&mut self) -> Result<Duration, wgpu::SurfaceError> {
        let now = Instant::now();

        let part_querier = self.parts.read().unwrap();

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let (target_view, resolve_target) = if let Some(texture) = self.framebuffer_texture.as_ref()
        {
            (&texture.view, Some(&view))
        } else {
            (&view, None)
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Command Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 0.0,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            self.pipelines.render::<_, _>(
                &mut pass,
                &self.projection,
                &*part_querier,
                &self.animated_model.display_list,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(now.elapsed())
    }

    pub fn get_subparts(&self) -> Vec<(Uuid, String)> {
        if let Some(model) = &self.model {
            let mut result = model
                .object_groups
                .iter()
                .map(|(k, v)| (*k, v.name.clone()))
                .collect::<Vec<_>>();
            result.sort_by(|a, b| a.1.cmp(&b.1));
            result
        } else {
            Vec::new()
        }
    }

    pub fn set_render_target(&mut self, group_id: Option<Uuid>) {
        if let Some(model) = &mut self.model {
            self.animated_model = AnimatedModel::from_model(
                model,
                group_id,
                &self.device,
                &self.queue,
                &self.colors,
                false,
            );

            let bounding_box =
                calculate_model_bounding_box(model, group_id, &*self.parts.read().unwrap());
            let center = bounding_box.center();

            let mut orbit_controller = self.orbit_controller.borrow_mut();
            orbit_controller.camera.look_at = Point3::new(center.x, center.y, center.z);
            orbit_controller.radius = (bounding_box.len_x() * bounding_box.len_x()
                + bounding_box.len_y() * bounding_box.len_y()
                + bounding_box.len_z() * bounding_box.len_z())
            .sqrt()
                * 2.0;
        }
    }

    pub fn state(&self) -> State {
        self.animated_model.state
    }

    pub fn handle_window_event(&mut self, event: event::WindowEvent, current_time: f32) -> bool {
        match event {
            event::WindowEvent::Resized(size) => {
                self.resize(size);
            }
            event::WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                self.resize(*new_inner_size);
            }
            event::WindowEvent::KeyboardInput { input, .. } => {
                if input.virtual_keycode == Some(event::VirtualKeyCode::Space)
                    && input.state == event::ElementState::Pressed
                {
                    self.advance(current_time);
                }
            }
            event::WindowEvent::MouseInput { state, button, .. } => {
                if button == event::MouseButton::Left {
                    self.orbit_controller
                        .borrow_mut()
                        .on_mouse_press(state == event::ElementState::Pressed);
                }
            }
            event::WindowEvent::MouseWheel { delta, .. } => match delta {
                event::MouseScrollDelta::LineDelta(_x, y) => {
                    self.orbit_controller.borrow_mut().zoom(y);
                }
                event::MouseScrollDelta::PixelDelta(d) => {
                    self.orbit_controller.borrow_mut().zoom(d.y as f32 * 0.5);
                }
            },
            event::WindowEvent::Touch(touch) => match touch.phase {
                event::TouchPhase::Started => {
                    self.orbit_controller.borrow_mut().on_mouse_press(true)
                }
                event::TouchPhase::Ended | event::TouchPhase::Cancelled => {
                    self.orbit_controller.borrow_mut().on_mouse_press(false)
                }
                event::TouchPhase::Moved => self
                    .orbit_controller
                    .borrow_mut()
                    .on_mouse_move(touch.location.x as f32, touch.location.y as f32),
            },
            event::WindowEvent::TouchpadMagnify { delta, .. } => {
                self.orbit_controller.borrow_mut().zoom(delta as f32);
            }
            event::WindowEvent::CursorMoved { position, .. } => {
                self.orbit_controller
                    .borrow_mut()
                    .on_mouse_move(position.x as f32, position.y as f32);
            }
            _ => return false,
        }

        true
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }
}
