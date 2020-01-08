use std::cell::RefCell;
use std::rc::Rc;
use std::slice::from_raw_parts;
use std::vec::Vec;

use cgmath::{Deg, PerspectiveFov, Point3, Quaternion, Rad, Rotation3};
use glow::HasContext;
use ldraw::color::{ColorReference, Material, MaterialRegistry};
use ldraw::{Matrix4, Vector3, Vector4};
use ldraw_renderer::error::RendererError;
use ldraw_renderer::geometry::{BufferIndex, GroupKey, NativeBakedModel};
use ldraw_renderer::scene::{ProjectionParams, ShadingParams};
use ldraw_renderer::shader::{Bindable, ProgramManager};

fn cast_as_bytes<'a>(input: &'a [f32]) -> &'a [u8] {
    unsafe { from_raw_parts(input.as_ptr() as *const u8, input.len() * 4) }
}

pub struct TestRenderer<T: HasContext> {
    gl: Rc<RefCell<T>>,

    program_manager: ProgramManager<T>,

    default_material: Material,

    vao_mesh: T::VertexArray,
    vbo_mesh_vertices: Option<T::Buffer>,
    vbo_mesh_normals: Option<T::Buffer>,
    vao_edge: T::VertexArray,
    vbo_edge_vertices: Option<T::Buffer>,
    vbo_edge_colors: Option<T::Buffer>,

    edge_length: i32,
    mesh_index: BufferIndex,
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
        gl: Rc<RefCell<T>>,
    ) -> Result<TestRenderer<T>, RendererError> {
        let program_manager = ProgramManager::new(Rc::clone(&gl))?;

        let default_material = ColorReference::resolve(7, &colors)
            .get_material()
            .unwrap()
            .clone();

        let vao_mesh;
        let vbo_mesh_vertices;
        let vbo_mesh_normals;
        let vao_edge;
        let vbo_edge_vertices;
        let vbo_edge_colors;

        let gl_ = &gl.borrow();

        unsafe {
            gl_.clear_color(1.0, 1.0, 1.0, 1.0);
            gl_.cull_face(glow::BACK);
            gl_.enable(glow::CULL_FACE);
            gl_.enable(glow::DEPTH_TEST);
            gl_.enable(glow::BLEND);
            gl_.depth_func(glow::LEQUAL);
            gl_.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            vao_mesh = gl_.create_vertex_array().unwrap();
            vbo_mesh_vertices = Some(gl_.create_buffer().unwrap());
            vbo_mesh_normals = Some(gl_.create_buffer().unwrap());
            gl_.bind_vertex_array(Some(vao_mesh));
            gl_.bind_buffer(glow::ARRAY_BUFFER, vbo_mesh_vertices);
            gl_.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(model.buffer.mesh.vertices.as_ref()),
                glow::STATIC_DRAW,
            );
            gl_.bind_buffer(glow::ARRAY_BUFFER, vbo_mesh_normals);
            gl_.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(model.buffer.mesh.normals.as_ref()),
                glow::STATIC_DRAW,
            );

            vao_edge = gl_.create_vertex_array().unwrap();
            vbo_edge_vertices = Some(gl_.create_buffer().unwrap());
            vbo_edge_colors = Some(gl_.create_buffer().unwrap());
            gl_.bind_buffer(glow::ARRAY_BUFFER, vbo_edge_vertices);
            gl_.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(model.buffer.edges.vertices.as_ref()),
                glow::STATIC_DRAW,
            );
            gl_.bind_buffer(glow::ARRAY_BUFFER, vbo_edge_colors);
            gl_.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(model.buffer.edges.colors.as_ref()),
                glow::STATIC_DRAW,
            );
        }

        let mut drawing_order = model
            .mesh_index
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

            vao_mesh,
            vbo_mesh_vertices,
            vbo_mesh_normals,
            vao_edge,
            vbo_edge_vertices,
            vbo_edge_colors,

            mesh_index: model.mesh_index.clone(),
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

        let gl = &self.gl.borrow();
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

        self.projection_params.update();
    }

    pub fn render(&mut self) {
        let gl = &self.gl.borrow();

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

                if let Some(e) = program.attrib_position {
                    gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_mesh_vertices);
                    gl.vertex_attrib_pointer_f32(e, 3, glow::FLOAT, false, 0, 0);
                }
                if let Some(e) = program.attrib_normal {
                    gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_mesh_normals);
                    gl.vertex_attrib_pointer_f32(e, 3, glow::FLOAT, false, 0, 0);
                }

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

                let index = self.mesh_index.0[order];

                gl.draw_arrays(glow::TRIANGLES, index.0 as i32, (index.1 - index.0) as i32);

                program.unbind();
            }

            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);

            let program = &self.program_manager.edge;
            program.bind();
            program.bind_uniforms(&self.projection_params);

            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_edge_vertices);
            gl.vertex_attrib_pointer_f32(
                program.attrib_position as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_edge_colors);
            gl.vertex_attrib_pointer_f32(program.attrib_color as u32, 3, glow::FLOAT, false, 0, 0);

            gl.draw_arrays(glow::LINES, 0, self.edge_length);

            program.unbind();

            gl.flush();
        }
    }
}

impl<T: HasContext> Drop for TestRenderer<T> {
    fn drop(&mut self) {
        let gl = &self.gl.borrow();

        unsafe {
            gl.delete_vertex_array(self.vao_mesh);
            gl.delete_vertex_array(self.vao_edge);

            gl.delete_buffer(self.vbo_mesh_vertices.unwrap());
            gl.delete_buffer(self.vbo_mesh_normals.unwrap());
            gl.delete_buffer(self.vbo_edge_vertices.unwrap());
            gl.delete_buffer(self.vbo_edge_colors.unwrap());
        }
    }
}
