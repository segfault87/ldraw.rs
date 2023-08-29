use std::ops::Range;

use image::GenericImageView;
use ldraw::Vector3;
use wgpu::util::DeviceExt;

use super::{
    camera::Projection,
    display_list::{DisplayList, Instances},
    part::{EdgeBuffer, MeshBuffer, OptionalEdgeBuffer, Part, PartQuerier},
};

pub struct MaterialUniformData {
    diffuse: Vector3,
    emissive: Vector3,
    roughness: f32,
    metalness: f32,
}

impl Default for MaterialUniformData {
    fn default() -> Self {
        Self {
            diffuse: Vector3::new(1.0, 1.0, 1.0),
            emissive: Vector3::new(0.0, 0.0, 0.0),
            roughness: 0.3,
            metalness: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RawMaterialUniformData {
    diffuse: [f32; 3],
    _padding0: [u8; 4],
    emissive: [f32; 3],
    roughness: f32,
    metalness: f32,
    _padding1: [u8; 12],
}

impl From<&MaterialUniformData> for RawMaterialUniformData {
    fn from(v: &MaterialUniformData) -> Self {
        Self {
            diffuse: [v.diffuse.x, v.diffuse.y, v.diffuse.z],
            _padding0: [0; 4],
            emissive: [v.emissive.x, v.emissive.y, v.emissive.z],
            roughness: v.roughness,
            metalness: v.metalness,
            _padding1: [0; 12],
        }
    }
}

impl RawMaterialUniformData {
    fn update(&mut self, data: &MaterialUniformData) {
        self.diffuse = data.diffuse.into();
        self.emissive = data.emissive.into();
        self.roughness = data.roughness;
        self.metalness = data.metalness;
    }
}

pub struct ShadingUniforms {
    pub bind_group: wgpu::BindGroup,

    pub material_data: MaterialUniformData,
    material_buffer: wgpu::Buffer,
    material_raw: RawMaterialUniformData,

    _env_map_texture_view: wgpu::TextureView,
    _env_map_sampler: wgpu::Sampler,
}

impl ShadingUniforms {
    pub fn new(
        device: &wgpu::Device,
        env_map_texture_view: wgpu::TextureView,
        env_map_sampler: wgpu::Sampler,
    ) -> Self {
        let material_data = MaterialUniformData::default();
        let material_raw = RawMaterialUniformData::from(&material_data);
        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform buffer for materials"),
            contents: bytemuck::cast_slice(&[material_raw]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind group for shading"),
            layout: &device.create_bind_group_layout(&Self::desc()),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&env_map_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&env_map_sampler),
                },
            ],
        });

        Self {
            bind_group,

            material_data,
            material_buffer,
            material_raw,

            _env_map_texture_view: env_map_texture_view,
            _env_map_sampler: env_map_sampler,
        }
    }

    pub fn update_materials(&mut self, queue: &wgpu::Queue) {
        self.material_raw.update(&self.material_data);

        queue.write_buffer(
            &self.material_buffer,
            0 as wgpu::BufferAddress,
            bytemuck::cast_slice(&[self.material_raw]),
        );
    }

    pub fn desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind group descriptor for shading"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        }
    }
}

pub struct DefaultMeshRenderingPipeline {
    pipeline: wgpu::RenderPipeline,
    pub shading_uniforms: ShadingUniforms,
}

impl DefaultMeshRenderingPipeline {
    fn load_envmap(device: &wgpu::Device, queue: &wgpu::Queue) -> (wgpu::Texture, wgpu::Sampler) {
        let image = image::load_from_memory_with_format(
            include_bytes!("../assets/env_cubemap.png"),
            image::ImageFormat::Png,
        )
        .unwrap();
        let rgba = image.to_rgba8();
        let (width, height) = image.dimensions();

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Environment map"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                aspect: wgpu::TextureAspect::All,
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        (texture, sampler)
    }

    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex shader for default mesh"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/model_vertex.wgsl").into()),
        });
        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment shader for default mesh"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/model_fragment_base.wgsl").into(),
            ),
        });

        let projection_bind_group_layout = device.create_bind_group_layout(&Projection::desc());

        let (env_map_texture, env_map_sampler) = Self::load_envmap(device, queue);
        let env_map_texture_view =
            env_map_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let shading_uniforms = ShadingUniforms::new(device, env_map_texture_view, env_map_sampler);
        let shading_bind_group_layout = device.create_bind_group_layout(&ShadingUniforms::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for default mesh"),
                bind_group_layouts: &[&projection_bind_group_layout, &shading_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for default meshes"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[MeshBuffer::desc(), Instances::<i32, i32>::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment_shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            pipeline: render_pipeline,
            shading_uniforms,
        }
    }

    pub fn render<'rp, K, G>(
        &'rp self,
        pass: &mut wgpu::RenderPass<'rp>,
        projection: &'rp Projection,
        part: &'rp Part,
        instances: &'rp Instances<K, G>,
        range: Range<u32>,
    ) {
        pass.set_vertex_buffer(0, part.mesh.vertices.slice(..));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &projection.bind_group, &[]);
        pass.set_bind_group(1, &self.shading_uniforms.bind_group, &[]);
        pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
        pass.set_index_buffer(part.mesh.indices.slice(..), part.mesh.index_format);
        pass.draw_indexed(range, 0, instances.range());
    }
}

pub struct NoShadingMeshRenderingPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl NoShadingMeshRenderingPipeline {
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex shader for default mesh without shading"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/model_vertex.wgsl").into()),
        });
        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment shader for default mesh"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/model_fragment_no_shading.wgsl").into(),
            ),
        });

        let projection_bind_group_layout = device.create_bind_group_layout(&Projection::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for default mesh without shading"),
                bind_group_layouts: &[&projection_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for default mesh without shading"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[MeshBuffer::desc(), Instances::<i32, i32>::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment_shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            pipeline: render_pipeline,
        }
    }

    pub fn render<'rp, K, G>(
        &'rp self,
        pass: &mut wgpu::RenderPass<'rp>,
        projection: &'rp Projection,
        part: &'rp Part,
        instances: &'rp Instances<K, G>,
        range: Range<u32>,
    ) {
        pass.set_vertex_buffer(0, part.mesh.vertices.slice(..));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &projection.bind_group, &[]);
        pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
        pass.set_index_buffer(part.mesh.indices.slice(..), part.mesh.index_format);
        pass.draw_indexed(range, 0, instances.range());
    }
}

pub struct EdgeRenderingPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl EdgeRenderingPipeline {
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex shader for edges"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/edge_vertex.wgsl").into()),
        });
        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment shader for edges"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/edge_fragment.wgsl").into()),
        });

        let projection_bind_group_layout = device.create_bind_group_layout(&Projection::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for optional edges"),
                bind_group_layouts: &[&projection_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for optional edges"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[EdgeBuffer::desc(), Instances::<i32, i32>::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment_shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Line,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            pipeline: render_pipeline,
        }
    }

    pub fn render<'p, K, G>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        projection: &'p Projection,
        part: &'p Part,
        instances: &'p Instances<K, G>,
    ) -> bool {
        if let Some(edges) = part.edges.as_ref() {
            pass.set_vertex_buffer(0, edges.vertices.slice(..));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &projection.bind_group, &[]);
            pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
            pass.set_index_buffer(edges.indices.slice(..), edges.index_format);
            pass.draw_indexed(edges.range.clone(), 0, instances.range());
            true
        } else {
            false
        }
    }
}

pub struct OptionalEdgeRenderingPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl OptionalEdgeRenderingPipeline {
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex shader for optional edges"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/optional_edge_vertex.wgsl").into(),
            ),
        });
        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment shader for optional edges"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/optional_edge_fragment.wgsl").into(),
            ),
        });

        let projection_bind_group_layout = device.create_bind_group_layout(&Projection::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for optional edges"),
                bind_group_layouts: &[&projection_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for optional edges"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[OptionalEdgeBuffer::desc(), Instances::<i32, i32>::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment_shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Line,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            pipeline: render_pipeline,
        }
    }

    pub fn render<'rp, K, G>(
        &'rp self,
        pass: &mut wgpu::RenderPass<'rp>,
        projection: &'rp Projection,
        part: &'rp Part,
        instances: &'rp Instances<K, G>,
    ) -> bool {
        if let Some(ref optional_edges) = part.optional_edges {
            pass.set_vertex_buffer(0, optional_edges.vertices.slice(..));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &projection.bind_group, &[]);
            pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
            pass.draw(optional_edges.range.clone(), instances.range());
            true
        } else {
            false
        }
    }
}

pub struct RenderingPipelineManager {
    pub mesh_default: DefaultMeshRenderingPipeline,
    pub mesh_no_shading: NoShadingMeshRenderingPipeline,
    pub edge: EdgeRenderingPipeline,
    pub optional_edge: OptionalEdgeRenderingPipeline,
}

impl RenderingPipelineManager {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        Self {
            mesh_default: DefaultMeshRenderingPipeline::new(device, queue, config),
            mesh_no_shading: NoShadingMeshRenderingPipeline::new(device, config),
            edge: EdgeRenderingPipeline::new(device, config),
            optional_edge: OptionalEdgeRenderingPipeline::new(device, config),
        }
    }

    pub fn render<'rp, K, G>(
        &'rp mut self,
        pass: &mut wgpu::RenderPass<'rp>,
        projection: &'rp Projection,
        part_querier: &'rp impl PartQuerier<G>,
        display_list: &'rp DisplayList<K, G>,
    ) -> u32 {
        let mut draws = 0;

        // Render opaque items first
        for (group, is_translucent, instances) in display_list.iter() {
            if let Some(part) = part_querier.get(group) {
                if let Some(range) = &part.mesh.colored_opaque_range {
                    self.mesh_default
                        .render(pass, projection, part, instances, range.clone());
                    draws += 1;
                }
                if let Some(range) = &part.mesh.colored_opaque_without_bfc_range {
                    self.mesh_no_shading
                        .render(pass, projection, part, instances, range.clone());
                    draws += 1;
                }
                if !is_translucent {
                    if let Some(range) = &part.mesh.uncolored_range {
                        self.mesh_default
                            .render(pass, projection, part, instances, range.clone());
                        draws += 1;
                    }
                    if let Some(range) = &part.mesh.uncolored_without_bfc_range {
                        self.mesh_no_shading.render(
                            pass,
                            projection,
                            part,
                            instances,
                            range.clone(),
                        );
                        draws += 1;
                    }
                    if self.edge.render(pass, projection, part, instances) {
                        draws += 1;
                    }
                    if self.optional_edge.render(pass, projection, part, instances) {
                        draws += 1;
                    }
                }
            }
        }
        // Then translucent items
        for (group, is_translucent, instances) in display_list.iter() {
            if let Some(part) = part_querier.get(group) {
                if let Some(range) = &part.mesh.colored_translucent_range {
                    self.mesh_default
                        .render(pass, projection, part, instances, range.clone());
                    draws += 1;
                }
                if let Some(range) = &part.mesh.colored_translucent_without_bfc_range {
                    self.mesh_no_shading
                        .render(pass, projection, part, instances, range.clone());
                    draws += 1;
                }
                if is_translucent {
                    if let Some(range) = &part.mesh.uncolored_range {
                        self.mesh_default
                            .render(pass, projection, part, instances, range.clone());
                        draws += 1;
                    }
                    if let Some(range) = &part.mesh.uncolored_without_bfc_range {
                        self.mesh_no_shading.render(
                            pass,
                            projection,
                            part,
                            instances,
                            range.clone(),
                        );
                        draws += 1;
                    }
                    if self.edge.render(pass, projection, part, instances) {
                        draws += 1;
                    }
                    if self.optional_edge.render(pass, projection, part, instances) {
                        draws += 1;
                    }
                }
            }
        }

        draws
    }
}
