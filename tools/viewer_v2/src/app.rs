use std::{
    collections::HashMap,
    rc::Rc,
    sync::{Arc, RwLock},
};

use cgmath::{Matrix, PerspectiveFov, SquareMatrix};
use ldraw::{
    color::ColorCatalog,
    document::MultipartDocument,
    error::ResolutionError,
    library::{resolve_dependencies_multipart, LibraryLoader, PartCache},
    Matrix3, Matrix4, PartAlias, Point3, Vector3,
};
use ldraw_ir::{model::Model, part::bake_part_from_multipart_document};
use ldraw_renderer::{
    camera::Projection,
    display_list::DisplayList,
    part::{Part, PartQuerier},
    pipeline::RenderingPipelineManager,
};
use winit::window::Window;

use super::texture::Texture;

#[derive(Default)]
struct SimplePartsPool(pub HashMap<PartAlias, Part>);

impl PartQuerier<PartAlias> for SimplePartsPool {
    fn get(&self, key: &PartAlias) -> Option<&Part> {
        self.0.get(key)
    }
}

fn truncate_matrix4(m: &Matrix4) -> Matrix3 {
    Matrix3::new(
        m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2],
    )
}

const SAMPLE_COUNT: u32 = 4;

pub struct App {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Window,

    framebuffer_texture: Texture,
    depth_texture: Texture,

    projection: Projection,
    pipelines: RenderingPipelineManager,

    loader: Rc<dyn LibraryLoader>,
    colors: Rc<ColorCatalog>,

    display_list: DisplayList<uuid::Uuid, PartAlias>,
    parts: Arc<RwLock<SimplePartsPool>>,

    tick: f32,
}

impl App {
    pub async fn new(
        window: Window,
        loader: Rc<dyn LibraryLoader>,
        colors: Rc<ColorCatalog>,
    ) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });

        let surface = unsafe { instance.create_surface(&window) }.unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::POLYGON_MODE_LINE,
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .unwrap();

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
            view_formats: vec![wgpu::TextureFormat::Bgra8UnormSrgb],
        };
        surface.configure(&device, &config);

        let framebuffer_texture = Texture::create_framebuffer(
            &device,
            &config,
            SAMPLE_COUNT,
            Some("Multisample framebuffer"),
        );
        let depth_texture =
            Texture::create_depth_texture(&device, &config, SAMPLE_COUNT, Some("Depth texture"));

        let mut projection = Projection::new(&device);
        projection.update(&queue);

        let pipelines = RenderingPipelineManager::new(&device, &queue, &config);

        let display_list = DisplayList::new();

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,

            framebuffer_texture,
            depth_texture,

            pipelines,

            projection,

            loader,
            colors,
            parts: Arc::new(RwLock::new(Default::default())),

            display_list,
            tick: 0.0,
        }
    }

    pub async fn set_document(
        &mut self,
        cache: Arc<RwLock<PartCache>>,
        document: &MultipartDocument,
    ) -> Result<(), ResolutionError> {
        let resolution_result = resolve_dependencies_multipart(
            document,
            Arc::clone(&cache),
            &self.colors,
            &*self.loader,
            &|alias, result| {
                println!("{}: {:?}", alias, result);
            },
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
                                    &self.device,
                                    &self.colors,
                                    &bake_part_from_multipart_document(
                                        part,
                                        &resolution_result,
                                        local,
                                    ),
                                ),
                            )
                        })
                    }),
            );

        let model = Model::from_ldraw_multipart_document(
            document,
            &self.colors,
            Some((&*self.loader, Arc::clone(&cache))),
        )
        .await;

        self.display_list =
            DisplayList::from_model(&model, &self.colors, &self.device, &self.queue);

        Ok(())
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let part_querier = self.parts.read().unwrap();

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Command Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.framebuffer_texture.view,
                    resolve_target: Some(&view),
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
                &self.display_list,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn update_camera(&mut self) {
        let aspect_ratio = self.size.width as f32 / self.size.height as f32;

        let projection = Matrix4::from(PerspectiveFov {
            fovy: cgmath::Rad::from(cgmath::Deg(60.0)),
            aspect: aspect_ratio,
            near: 10.0,
            far: 1_000_000.0,
        });

        let view_matrix = Matrix4::look_at_rh(
            Point3::new(self.tick.sin() * 700.0, -250.0, self.tick.cos() * 700.0),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, -1.0, 0.0),
        );

        let model_view = view_matrix;

        let normal_matrix = truncate_matrix4(&model_view)
            .invert()
            .unwrap_or_else(Matrix3::identity)
            .transpose();

        self.projection.data.projection_matrix = projection;
        self.projection.data.view_matrix = view_matrix;
        self.projection.data.model_view_matrix = model_view;
        self.projection.data.normal_matrix = normal_matrix;

        self.projection.update(&self.queue);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            self.update_camera();

            self.framebuffer_texture = Texture::create_framebuffer(
                &self.device,
                &self.config,
                SAMPLE_COUNT,
                Some("Multisample framebuffer"),
            );
            self.depth_texture = Texture::create_depth_texture(
                &self.device,
                &self.config,
                SAMPLE_COUNT,
                Some("Depth texture"),
            );
        }
    }

    pub fn update(&mut self) {
        self.update_camera();
        self.tick += 1.0 / 120.0;
    }
}
