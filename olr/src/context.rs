use image::RgbaImage;
use ldraw::Vector2;
use ldraw_ir::geometry::BoundingBox2;
use ldraw_renderer::{camera::Projection, pipeline::RenderingPipelineManager};

use crate::error::ContextCreationError;

pub struct OlrContext {
    pub width: u32,
    pub height: u32,

    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,

    pub(super) pipelines: RenderingPipelineManager,
    pub(super) projection: Projection,

    pub(super) framebuffer_texture: wgpu::Texture,
    pub(super) framebuffer_texture_view: wgpu::TextureView,

    pub(super) _depth_texture: wgpu::Texture,
    pub(super) depth_texture_view: wgpu::TextureView,

    output_buffer: wgpu::Buffer,
}

impl OlrContext {
    pub async fn new(
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Result<Self, ContextCreationError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok_or(ContextCreationError::NoAdapterFound)?;

        let (device, queue) = adapter.request_device(&Default::default(), None).await?;

        let framebuffer_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let framebuffer_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: framebuffer_format,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            label: Some("Render framebuffer"),
            view_formats: &[],
        });
        let framebuffer_texture_view = framebuffer_texture.create_view(&Default::default());

        let pipelines = RenderingPipelineManager::new(&device, &queue, framebuffer_format, true, 4);
        let projection = Projection::new(&device);

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            label: Some("Depth buffer"),
            view_formats: &[],
        });
        let depth_texture_view = depth_texture.create_view(&Default::default());

        let output_buffer_size =
            (std::mem::size_of::<u32>() as u32 * width * height) as wgpu::BufferAddress;
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Output buffer"),
            mapped_at_creation: false,
        });

        Ok(Self {
            width,
            height,

            device,
            queue,

            pipelines,
            projection,

            framebuffer_texture,
            framebuffer_texture_view,

            _depth_texture: depth_texture,
            depth_texture_view,

            output_buffer,
        })
    }

    pub async fn finish(
        &self,
        mut encoder: wgpu::CommandEncoder,
        bounds: Option<BoundingBox2>,
    ) -> RgbaImage {
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &self.framebuffer_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::ImageCopyBuffer {
                buffer: &self.output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(std::mem::size_of::<u32>() as u32 * self.width),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(Some(encoder.finish()));

        let pixels = {
            let buffer_slice = self.output_buffer.slice(..);

            let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
            self.device.poll(wgpu::Maintain::Wait);
            rx.receive().await.unwrap().unwrap();

            buffer_slice.get_mapped_range()
        };

        let bounds = bounds
            .unwrap_or_else(|| BoundingBox2::new(&Vector2::new(0.0, 0.0), &Vector2::new(1.0, 1.0)));

        let x1 = (bounds.min.x * self.width as f32) as usize;
        let y1 = (bounds.min.y * self.height as f32) as usize;
        let x2 = (bounds.max.x * self.width as f32) as usize;
        let y2 = (bounds.max.y * self.height as f32) as usize;
        let cw = x2 - x1;
        let ch = y2 - y1;

        let mut pixels_rearranged: Vec<u8> = Vec::new();
        for v in (y1..y2).rev() {
            let s = 4 * v * self.width as usize + x1 * 4;
            pixels_rearranged.extend_from_slice(&pixels[s..(s + (cw * 4))]);
        }

        self.output_buffer.unmap();

        RgbaImage::from_raw(cw as _, ch as _, pixels_rearranged).unwrap()
    }
}
