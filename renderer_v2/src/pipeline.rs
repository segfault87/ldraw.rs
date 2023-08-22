use ldraw::{color::ColorReference, Vector4};
use wgpu::util::DeviceExt;

use super::{
    camera::Projection,
    display_list::Instances,
    part::{EdgeBuffer, MeshBuffer, OptionalEdgeBuffer, Part},
};

struct ColorUniformData {
    color: Vector4,
    edge_color: Vector4,
    use_instanced_colors: bool,
}

impl Default for ColorUniformData {
    fn default() -> Self {
        Self {
            color: Vector4::new(0.0, 0.0, 0.0, 1.0),
            edge_color: Vector4::new(0.4, 0.4, 0.4, 1.0),
            use_instanced_colors: true,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RawColorUniformData {
    color: [f32; 4],
    edge_color: [f32; 4],
    use_instanced_color: i32,
    _padding: [u8; 12],
}

impl From<&ColorUniformData> for RawColorUniformData {
    fn from(d: &ColorUniformData) -> Self {
        Self {
            color: d.color.clone().into(),
            edge_color: d.edge_color.clone().into(),
            use_instanced_color: if d.use_instanced_colors { 1 } else { 0 },
            _padding: [0; 12],
        }
    }
}

impl RawColorUniformData {
    pub fn update(&mut self, data: &ColorUniformData) {
        self.color = data.color.clone().into();
        self.edge_color = data.color.clone().into();
        self.use_instanced_color = if data.use_instanced_colors { 1 } else { 0 };
    }
}

pub struct ColorUniforms {
    pub bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,

    data: ColorUniformData,
    raw: RawColorUniformData,
}

impl ColorUniforms {
    pub fn new(device: &wgpu::Device) -> Self {
        let data = ColorUniformData::default();
        let raw = RawColorUniformData::from(&data);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform buffer for colors"),
            contents: bytemuck::cast_slice(&[raw]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind group for colors"),
            layout: &device.create_bind_group_layout(&Self::desc()),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            uniform_buffer,
            bind_group,
            data,
            raw,
        }
    }

    pub fn set_color(&mut self, queue: &wgpu::Queue, color: &ColorReference) {
        if let Some(c) = color.get_color_rgba() {
            self.data.color = c;
        }
        if let Some(c) = color.get_edge_color_rgba() {
            self.data.edge_color = c;
        }
        self.update(queue);
    }

    pub fn set_use_instanced_color(&mut self, queue: &wgpu::Queue, use_instanced_color: bool) {
        self.data.use_instanced_colors = use_instanced_color;
        self.update(queue);
    }

    fn update(&mut self, queue: &wgpu::Queue) {
        self.raw.update(&self.data);

        queue.write_buffer(
            &self.uniform_buffer,
            0 as wgpu::BufferAddress,
            bytemuck::cast_slice(&[self.raw]),
        );
    }

    pub fn desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind group descriptor for color uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        }
    }
}

pub struct DefaultMeshRenderingPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl DefaultMeshRenderingPipeline {
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
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
        let color_bind_group_layout = device.create_bind_group_layout(&ColorUniforms::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for default mesh"),
                bind_group_layouts: &[&projection_bind_group_layout, &color_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for default meshes"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[MeshBuffer::desc(), Instances::<i32>::desc()],
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
        }
    }

    pub fn render<'p, K>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        projection: &'p Projection,
        colors: &'p ColorUniforms,
        part: &'p Part,
        instances: &'p Instances<K>,
    ) {
        if let Some(range) = part.mesh.uncolored_range.as_ref() {
            pass.set_vertex_buffer(0, part.mesh.vertices.slice(..));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &projection.bind_group, &[]);
            pass.set_bind_group(1, &colors.bind_group, &[]);
            pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
            pass.set_index_buffer(part.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(range.clone(), 0, instances.range());
        }
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
        let color_bind_group_layout = device.create_bind_group_layout(&ColorUniforms::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for default mesh without shading"),
                bind_group_layouts: &[&projection_bind_group_layout, &color_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for default mesh without shading"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[MeshBuffer::desc(), Instances::<i32>::desc()],
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

    pub fn render<'p, K>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        projection: &'p Projection,
        colors: &'p ColorUniforms,
        part: &'p Part,
        instances: &'p Instances<K>,
    ) {
        if let Some(range) = part.mesh.uncolored_range.as_ref() {
            pass.set_vertex_buffer(0, part.mesh.vertices.slice(..));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &projection.bind_group, &[]);
            pass.set_bind_group(1, &colors.bind_group, &[]);
            pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
            pass.set_index_buffer(part.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(range.clone(), 0, instances.range());
        }
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
        let color_bind_group_layout = device.create_bind_group_layout(&ColorUniforms::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for optional edges"),
                bind_group_layouts: &[&projection_bind_group_layout, &color_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for optional edges"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[EdgeBuffer::desc(), Instances::<i32>::desc()],
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

    pub fn render<'p, K>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        projection: &'p Projection,
        colors: &'p ColorUniforms,
        part: &'p Part,
        instances: &'p Instances<K>,
    ) {
        if let Some(ref edges) = part.edges.as_ref() {
            pass.set_vertex_buffer(0, edges.vertices.slice(..));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &projection.bind_group, &[]);
            pass.set_bind_group(1, &colors.bind_group, &[]);
            pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
            pass.set_index_buffer(edges.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(edges.range.clone(), 0, instances.range());
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
        let color_bind_group_layout = device.create_bind_group_layout(&ColorUniforms::desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for optional edges"),
                bind_group_layouts: &[&projection_bind_group_layout, &color_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render pipeline for optional edges"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs",
                buffers: &[OptionalEdgeBuffer::desc(), Instances::<i32>::desc()],
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

    pub fn render<'p, K>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        projection: &'p Projection,
        colors: &'p ColorUniforms,
        part: &'p Part,
        instances: &'p Instances<K>,
    ) {
        if let Some(optional_edges) = part.optional_edges.as_ref() {
            pass.set_vertex_buffer(0, optional_edges.vertices.slice(..));
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &projection.bind_group, &[]);
            pass.set_bind_group(1, &colors.bind_group, &[]);
            pass.set_vertex_buffer(1, instances.instance_buffer.slice(..));
            pass.draw(optional_edges.range.clone(), instances.range());
        }
    }
}

pub struct RenderingPipelineManager {
    pub color_uniforms: ColorUniforms,

    pub mesh_default: DefaultMeshRenderingPipeline,
    pub mesh_no_shading: NoShadingMeshRenderingPipeline,
    pub edge: EdgeRenderingPipeline,
    pub optional_edge: OptionalEdgeRenderingPipeline,
}

impl RenderingPipelineManager {
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        Self {
            color_uniforms: ColorUniforms::new(device),

            mesh_default: DefaultMeshRenderingPipeline::new(device, config),
            mesh_no_shading: NoShadingMeshRenderingPipeline::new(device, config),
            edge: EdgeRenderingPipeline::new(device, config),
            optional_edge: OptionalEdgeRenderingPipeline::new(device, config),
        }
    }
}
