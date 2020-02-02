use std::{
    rc::Rc,
    vec::Vec,
};

use cgmath::{Deg, PerspectiveFov, Point3, Quaternion, Rad, Rotation3};
use glow::HasContext;
use ldraw::{
    color::{ColorReference, Material, MaterialRegistry},
    {Matrix4, Vector3, Vector4},
};
use ldraw_renderer::{
    error::RendererError,
    geometry::{GroupKey, NativeBakedModel, OpenGlBakedModel},
    scene::{ProjectionParams, ShadingParams},
    shader::{Bindable, ProgramManager},
};

pub struct TestRenderer<T: HasContext> {
    gl: Rc<T>,

    program_manager: ProgramManager<T>,

    default_material: Material,

    model: OpenGlBakedModel<T>,

    edge_length: i32,
    drawing_order: Vec<GroupKey>,

    center: Point3<f32>,
    radius: f32,
    degrees: Deg<f32>,

    projection_params: ProjectionParams,
    shading_params: ShadingParams,

    time: f32,
}

impl<T: HasContext> TestRenderer<T> {
    pub fn new(
        model: &NativeBakedModel,
        colors: &MaterialRegistry,
        gl: Rc<T>,
    ) -> Result<TestRenderer<T>, RendererError> {
        let program_manager = ProgramManager::new(Rc::clone(&gl))?;

        let default_material = ColorReference::resolve(7, &colors)
            .get_material()
            .unwrap()
            .clone();

        let gl_ = &gl;

        unsafe {
            gl_.clear_color(1.0, 1.0, 1.0, 1.0);
            gl_.cull_face(glow::BACK);
            gl_.enable(glow::CULL_FACE);
            gl_.enable(glow::DEPTH_TEST);
            gl_.enable(glow::BLEND);
            gl_.depth_func(glow::LEQUAL);
            gl_.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl_.line_width(1.0);
        }

        let opengl_model = OpenGlBakedModel::create(Rc::clone(&gl), &model);

        let mut drawing_order = model
            .index
            .0
            .keys()
            .map(|v| v.clone())
            .collect::<Vec<_>>();
        drawing_order.sort();

        let center = Point3::new(0.0, 0.0, 0.0);
        let radius = 500.0;
        let degrees = Deg(0.0);

        Ok(TestRenderer {
            gl: Rc::clone(&gl),

            program_manager,

            default_material,

            model: opengl_model,

            edge_length: model.buffer.edges.len() as i32,
            drawing_order,

            center,
            radius,
            degrees,

            projection_params: ProjectionParams::new(),
            shading_params: ShadingParams::new(),

            time: 0.0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.projection_params.projection = Matrix4::from(PerspectiveFov {
            fovy: Rad::from(Deg(45.0)),
            aspect: width as f32 / height as f32,
            near: 1.0,
            far: 100000.0,
        });

        let gl = &self.gl;
        unsafe {
            gl.viewport(0, 0, width as i32, height as i32);
        }
    }

    pub fn animate(&mut self, time: f32) {
        let delta = time - self.time;

        self.projection_params.view_matrix = Matrix4::look_at(
            Point3::new(
                0.0 + self.center.x,
                -self.radius / 5.0 * 2.0 + self.center.y,
                self.radius + self.center.z,
            ),
            self.center,
            Vector3::new(0.0, -1.0, 0.0),
        );

        self.degrees += Deg(delta * 60.0);
        let rotation = Quaternion::from_angle_y(self.degrees);
        self.projection_params.model_view =
            self.projection_params.view_matrix * Matrix4::from(rotation);

        self.time = time;
    }

    pub fn render(&mut self) {
        let gl = &self.gl;

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);

            for order in self.drawing_order.iter() {
                let color = if let Some(mat) = order.color_ref.get_material() {
                    Vector4::from(mat.color)
                } else {
                    Vector4::from(self.default_material.color)
                };
                let program = if order.bfc {
                    &self.program_manager.solid
                } else {
                    &self.program_manager.solid_flat
                };
                program.bind();
                program.bind_uniforms(&self.projection_params, &self.shading_params, &color);

                self.model.buffer.mesh.bind(&program.attrib_position, &program.attrib_normal);

                if order.color_ref.is_material()
                    && order
                        .color_ref
                        .get_material()
                        .unwrap()
                        .is_semi_transparent()
                {
                    gl.enable(glow::BLEND);
                } else {
                    gl.disable(glow::BLEND);
                }

                let index = self.model.index.0[order];

                gl.draw_arrays(glow::TRIANGLES, index.0 as i32, (index.1 - index.0) as i32);

                program.unbind();
            }

            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);

            let program = &self.program_manager.edge;
            program.bind();
            program.bind_uniforms(&self.projection_params);
            self.model.buffer.edges.bind(&program.attrib_position, &program.attrib_colors);

            gl.draw_arrays(glow::LINES, 0, self.edge_length);

            program.unbind();

            gl.flush();
        }
    }
}
