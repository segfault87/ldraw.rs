use cgmath::SquareMatrix;
use image::RgbaImage;
use ldraw::{
    color::{Color, ColorCatalog},
    Matrix4, PartAlias, Point3,
};
use ldraw_ir::{geometry::BoundingBox2, model::Model};
use ldraw_renderer::{
    camera::{OrthographicCamera, ViewBounds},
    display_list::DisplayList,
    part::{Part, PartQuerier},
    util::calculate_model_bounding_box,
};
use uuid::Uuid;

use crate::context::OlrContext;

pub struct Ops<'a> {
    context: &'a mut OlrContext,

    encoder: wgpu::CommandEncoder,
}

impl<'a> Ops<'a> {
    pub fn new(context: &'a mut OlrContext) -> Self {
        let encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Command Encoder for Offscreen"),
            });

        Self { context, encoder }
    }

    pub async fn render_single_part(mut self, part: &Part, color: &Color) -> RgbaImage {
        let mut render_pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Offscreen Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.context.framebuffer_texture_view,
                resolve_target: None,
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
                view: &self.context.depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        let camera = OrthographicCamera::new_isometric(
            Point3::new(0.0, 0.0, 0.0),
            ViewBounds::BoundingBox3(part.bounding_box.clone()),
        );
        self.context.projection.update_camera(
            &self.context.queue,
            &camera,
            (self.context.width, self.context.height).into(),
        );

        self.context.pipelines.render_single_part(
            &self.context.device,
            &self.context.queue,
            &mut render_pass,
            &self.context.projection,
            part,
            Matrix4::identity(),
            color,
        );

        drop(render_pass);

        let bounds = camera.view_bounds.project(
            &self.context.projection.data.get_model_view_matrix(),
            (self.context.width, self.context.height).into(),
        );

        self.finish(bounds).await
    }

    pub async fn render_model(
        mut self,
        model: &Model,
        group_id: Option<Uuid>,
        parts: &impl PartQuerier<PartAlias>,
        colors: &ColorCatalog,
    ) -> RgbaImage {
        let bounding_box = calculate_model_bounding_box(model, group_id, parts);
        let center = bounding_box.center();

        let camera = OrthographicCamera::new_isometric(
            Point3::new(center.x, center.y, center.z),
            ViewBounds::BoundingBox3(bounding_box),
        );

        let display_list = DisplayList::from_model(
            model,
            group_id,
            &self.context.device,
            &self.context.queue,
            colors,
        );

        {
            let mut render_pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Offscreen Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.context.framebuffer_texture_view,
                    resolve_target: None,
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
                    view: &self.context.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            self.context.projection.update_camera(
                &self.context.queue,
                &camera,
                (self.context.width, self.context.height).into(),
            );

            self.context.pipelines.render(
                &mut render_pass,
                &self.context.projection,
                parts,
                &display_list,
            );
        }

        let bounds = camera.view_bounds.project(
            &self.context.projection.data.get_model_view_matrix(),
            (self.context.width, self.context.height).into(),
        );

        self.finish(bounds).await
    }

    async fn finish(self, bounds: BoundingBox2) -> RgbaImage {
        self.context.finish(self.encoder, Some(bounds)).await
    }
}
