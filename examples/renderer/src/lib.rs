use std::cell::RefCell;
use std::rc::Rc;
use std::slice::from_raw_parts;
use std::vec::Vec;

use cgmath::{
    Deg, InnerSpace, Matrix, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix,
};
use glow::HasContext;
use ldraw::{Matrix3, Matrix4, Vector3, Vector4};
use ldraw::color::{ColorReference, Material, MaterialRegistry};
use ldraw_renderer::geometry::{BufferIndex, GroupKey, NativeBakedModel};

fn cast_as_bytes<'a>(input: &'a [f32]) -> &'a [u8] {
    unsafe { from_raw_parts(input.as_ptr() as *const u8, input.len() * 4) }
}

fn inv_mat3(src: &Matrix4) -> Matrix3 {
    let a00 = src[0][0];
    let a01 = src[0][1];
    let a02 = src[0][2];
    let a10 = src[1][0];
    let a11 = src[1][1];
    let a12 = src[1][2];
    let a20 = src[2][0];
    let a21 = src[2][1];
    let a22 = src[2][2];

    let b01 = a22 * a11 - a12 * a21;
    let b11 = -a22 * a10 + a12 * a20;
    let b21 = a21 * a10 - a11 * a20;

    let det = a00 * b01 + a01 * b11 + a02 * b21;
    if det == 0.0 {
        panic!("This matrix is not invertible.");
    }
    let id = 1.0 / det;

    Matrix3::new(
        b01 * id,
        (-a22 * a01 + a02 * a21) * id,
        (a12 * a01 - a02 * a11) * id,
        b11 * id,
        (a22 * a00 - a02 * a20) * id,
        (-a12 * a00 + a02 * a10) * id,
        b21 * id,
        (-a21 * a00 + a01 * a20) * id,
        (a11 * a00 - a01 * a10) * id,
    )
}

pub fn compile_shader<T: HasContext>(gl: &T, src: &str, ty: u32) -> Result<T::Shader, String> {
    let shader;

    unsafe {
        shader = gl.create_shader(ty).unwrap();

        gl.shader_source(shader, src);
        gl.compile_shader(shader);

        if !gl.get_shader_compile_status(shader) {
            Err(gl.get_shader_info_log(shader))
        } else {
            Ok(shader)
        }
    }
}

#[derive(Debug)]
pub struct Program<T: HasContext> {
    gl: Rc<RefCell<Box<T>>>,

    vs: T::Shader,
    fs: T::Shader,
    pub program: T::Program,
}

impl<T: HasContext> Program<T> {
    pub fn new(gl: Rc<RefCell<Box<T>>>, vs: &String, fs: &String) -> Result<Program<T>, String> {
        let gl_ = &**gl.borrow();

        let vs = compile_shader(gl_, &vs, glow::VERTEX_SHADER)?;
        let fs = compile_shader(gl_, &fs, glow::FRAGMENT_SHADER)?;

        unsafe {
            let program = gl_.create_program().unwrap();
            gl_.attach_shader(program, vs);
            gl_.attach_shader(program, fs);
            gl_.link_program(program);

            match gl_.get_program_link_status(program) {
                false => Err(gl_.get_program_info_log(program)),
                true => Ok(Program {
                    gl: Rc::clone(&gl),
                    vs,
                    fs,
                    program,
                }),
            }
        }
    }
}

impl<T: HasContext> Drop for Program<T> {
    fn drop(&mut self) {
        let gl = &**self.gl.borrow();

        unsafe {
            gl.delete_shader(self.vs);
            gl.delete_shader(self.fs);
            gl.delete_program(self.program);
        }
    }
}

pub struct TestRenderer<T: HasContext> {
    gl: Rc<RefCell<Box<T>>>,

    default_program: Program<T>,
    edge_program: Program<T>,

    default_material: Material,

    vao_mesh: T::VertexArray,
    vbo_mesh_vertices: Option<T::Buffer>,
    vbo_mesh_normals: Option<T::Buffer>,
    vao_edge: T::VertexArray,
    vbo_edge_vertices: Option<T::Buffer>,
    vbo_edge_colors: Option<T::Buffer>,

    uniform_edge_projection: Option<T::UniformLocation>,
    uniform_edge_model_view: Option<T::UniformLocation>,
    uniform_edge_view_matrix: Option<T::UniformLocation>,
    attribute_edge_position: i32,
    attribute_edge_color: i32,

    uniform_default_projection: Option<T::UniformLocation>,
    uniform_default_model_view: Option<T::UniformLocation>,
    uniform_default_view_matrix: Option<T::UniformLocation>,
    uniform_default_normal_matrix: Option<T::UniformLocation>,
    uniform_default_color: Option<T::UniformLocation>,
    uniform_default_is_bfc_certified: Option<T::UniformLocation>,
    uniform_default_light_color: Option<T::UniformLocation>,
    uniform_default_light_direction: Option<T::UniformLocation>,
    attribute_default_position: i32,
    attribute_default_normal: i32,

    edge_length: i32,
    mesh_index: BufferIndex,
    drawing_order: Vec<GroupKey>,

    center: Point3<f32>,
    radius: f32,
    degrees: Deg<f32>,
    projection: Matrix4,
    model_view: Matrix4,
    view_inversed: Matrix4,
    light_color: Vector4,
    light_direction: Vector4,

    time: f32,
}

impl<T: HasContext> TestRenderer<T> {
    pub fn new(
        model: &NativeBakedModel,
        colors: &MaterialRegistry,
        gl: Rc<RefCell<Box<T>>>,
        default_program: Program<T>,
        edge_program: Program<T>,
    ) -> TestRenderer<T> {
        let default_material = ColorReference::resolve(7, &colors).get_material().unwrap().clone();

        let vao_mesh;
        let vbo_mesh_vertices;
        let vbo_mesh_normals;
        let vao_edge;
        let vbo_edge_vertices;
        let vbo_edge_colors;

        let uniform_edge_projection;
        let uniform_edge_model_view;
        let uniform_edge_view_matrix;
        let attribute_edge_position;
        let attribute_edge_color;

        let uniform_default_projection;
        let uniform_default_model_view;
        let uniform_default_view_matrix;
        let uniform_default_normal_matrix;
        let uniform_default_color;
        let uniform_default_is_bfc_certified;
        let uniform_default_light_color;
        let uniform_default_light_direction;
        let attribute_default_position;
        let attribute_default_normal;

        let gl_ = &**gl.borrow();

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

            uniform_edge_projection = gl_.get_uniform_location(edge_program.program, "projection");
            uniform_edge_model_view = gl_.get_uniform_location(edge_program.program, "modelView");
            uniform_edge_view_matrix = gl_.get_uniform_location(edge_program.program, "viewMatrix");
            attribute_edge_position = gl_.get_attrib_location(edge_program.program, "position");
            attribute_edge_color = gl_.get_attrib_location(edge_program.program, "color");
            uniform_default_projection =
                gl_.get_uniform_location(default_program.program, "projection");
            uniform_default_model_view =
                gl_.get_uniform_location(default_program.program, "modelView");
            uniform_default_view_matrix =
                gl_.get_uniform_location(default_program.program, "viewMatrix");
            uniform_default_normal_matrix =
                gl_.get_uniform_location(default_program.program, "normalMatrix");
            uniform_default_color = gl_.get_uniform_location(default_program.program, "color");
            uniform_default_is_bfc_certified =
                gl_.get_uniform_location(default_program.program, "isBfcCertified");
            uniform_default_light_color =
                gl_.get_uniform_location(default_program.program, "lightColor");
            uniform_default_light_direction =
                gl_.get_uniform_location(default_program.program, "lightDirection");
            attribute_default_position =
                gl_.get_attrib_location(default_program.program, "position");
            attribute_default_normal = gl_.get_attrib_location(default_program.program, "normal");
        }

        let mut drawing_order = model
            .mesh_index
            .0
            .keys()
            .map(|v| v.clone())
            .collect::<Vec<_>>();
        drawing_order.sort();

        let projection = Matrix4::identity();
        let model_view = Matrix4::identity();
        let view_inversed = Matrix4::identity();

        let center = Point3::new(0.0, 0.0, 0.0);
        let radius = 500.0;
        let degrees = Deg(0.0);
        let light_color = Vector4::new(1.0, 1.0, 1.0, 1.0);
        let light_direction = Vector4::new(0.0, -0.5, 0.7, 1.0).normalize();

        TestRenderer {
            gl: Rc::clone(&gl),

            default_program,
            edge_program,

            default_material,

            vao_mesh,
            vbo_mesh_vertices,
            vbo_mesh_normals,
            vao_edge,
            vbo_edge_vertices,
            vbo_edge_colors,

            uniform_edge_projection,
            uniform_edge_model_view,
            uniform_edge_view_matrix,
            attribute_edge_position,
            attribute_edge_color,

            uniform_default_projection,
            uniform_default_model_view,
            uniform_default_view_matrix,
            uniform_default_normal_matrix,
            uniform_default_color,
            uniform_default_is_bfc_certified,
            uniform_default_light_color,
            uniform_default_light_direction,
            attribute_default_position,
            attribute_default_normal,

            mesh_index: model.mesh_index.clone(),
            edge_length: model.buffer.edges.len() as i32,
            drawing_order,

            center,
            radius,
            degrees,
            projection,
            model_view,
            view_inversed,
            light_color,
            light_direction,

            time: 0.0,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.projection = Matrix4::from(PerspectiveFov {
            fovy: Rad::from(Deg(45.0)),
            aspect: width as f32 / height as f32,
            near: 1.0,
            far: 100000.0,
        });

        let gl = &**self.gl.borrow();
        unsafe {
            gl.viewport(0, 0, width as i32, height as i32);
        }
    }

    pub fn animate(&mut self, time: f32) {
        let delta = time - self.time;

        let view = Matrix4::look_at(
            Point3::new(
                0.0 + self.center.x,
                -self.radius / 5.0 * 2.0 + self.center.y,
                self.radius + self.center.z,
            ),
            self.center,
            Vector3::new(0.0, -1.0, 0.0),
        );
        self.view_inversed = view.invert().unwrap();

        self.degrees += Deg(delta * 60.0);
        let rotation = Quaternion::from_angle_y(self.degrees);
        self.model_view = view * Matrix4::from(rotation);

        self.time = time;
    }

    pub fn render(&self) {
        let gl = &**self.gl.borrow();

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.use_program(Some(self.default_program.program));

            gl.enable_vertex_attrib_array(self.attribute_default_position as u32);
            gl.enable_vertex_attrib_array(self.attribute_default_normal as u32);
            gl.enable(glow::DEPTH_TEST);

            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_mesh_vertices);
            gl.vertex_attrib_pointer_f32(
                self.attribute_default_position as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_mesh_normals);
            gl.vertex_attrib_pointer_f32(
                self.attribute_default_normal as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.uniform_matrix_4_f32_slice(
                self.uniform_default_projection,
                false,
                self.projection.as_ref(),
            );
            gl.uniform_matrix_4_f32_slice(
                self.uniform_default_model_view,
                false,
                self.model_view.as_ref(),
            );
            gl.uniform_matrix_4_f32_slice(
                self.uniform_default_view_matrix,
                false,
                self.view_inversed.as_ref(),
            );
            let normal_matrix = inv_mat3(&self.model_view).transpose();
            gl.uniform_matrix_3_f32_slice(
                self.uniform_default_normal_matrix,
                false,
                normal_matrix.as_ref(),
            );

            for order in self.drawing_order.iter() {
                let color = if let Some(mat) = order.color_ref.get_material() {
                    Vector4::from(mat.color)
                } else {
                    Vector4::from(self.default_material.color)
                };
                gl.uniform_4_f32_slice(self.uniform_default_color, color.as_ref());
                if order.bfc {
                    gl.enable(glow::CULL_FACE);
                    gl.uniform_1_i32(self.uniform_default_is_bfc_certified, 1);
                } else {
                    gl.disable(glow::CULL_FACE);
                    gl.uniform_1_i32(self.uniform_default_is_bfc_certified, 0);
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
                gl.uniform_4_f32_slice(self.uniform_default_light_color, self.light_color.as_ref());
                gl.uniform_4_f32_slice(
                    self.uniform_default_light_direction,
                    self.light_direction.as_ref(),
                );

                let index = self.mesh_index.0[order];

                gl.draw_arrays(glow::TRIANGLES, index.0 as i32, (index.1 - index.0) as i32);
            }

            gl.disable(glow::BLEND);
            gl.disable(glow::CULL_FACE);

            gl.disable_vertex_attrib_array(self.attribute_default_position as u32);
            gl.disable_vertex_attrib_array(self.attribute_default_normal as u32);

            gl.use_program(Some(self.edge_program.program));

            gl.enable_vertex_attrib_array(self.attribute_edge_position as u32);
            gl.enable_vertex_attrib_array(self.attribute_edge_color as u32);

            gl.uniform_matrix_4_f32_slice(
                self.uniform_edge_projection,
                false,
                self.projection.as_ref(),
            );
            gl.uniform_matrix_4_f32_slice(
                self.uniform_edge_model_view,
                false,
                self.model_view.as_ref(),
            );
            gl.uniform_matrix_4_f32_slice(
                self.uniform_edge_view_matrix,
                false,
                self.view_inversed.as_ref(),
            );

            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_edge_vertices);
            gl.vertex_attrib_pointer_f32(
                self.attribute_edge_position as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, self.vbo_edge_colors);
            gl.vertex_attrib_pointer_f32(
                self.attribute_edge_color as u32,
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );

            gl.draw_arrays(glow::LINES, 0, self.edge_length);

            gl.disable_vertex_attrib_array(self.attribute_edge_position as u32);
            gl.disable_vertex_attrib_array(self.attribute_edge_color as u32);

            gl.flush();
        }
    }
}

impl<T: HasContext> Drop for TestRenderer<T> {
    fn drop(&mut self) {
        let gl = &**self.gl.borrow();

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
