use std::ops::Range;

use super::{
    camera::Projection,
    display_list::{DisplayList, Instances},
    part::{EdgeBuffer, MeshBuffer, OptionalEdgeBuffer, Part, PartQuerier},
};

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

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render pipeline layout for default mesh"),
                bind_group_layouts: &[&projection_bind_group_layout],
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
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        Self {
            mesh_default: DefaultMeshRenderingPipeline::new(device, config),
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
