use std::{
    io::{BufWriter, Write as IoWrite},
    rc::Rc,
    str,
};

use cgmath::prelude::*;
use glow::HasContext;
use ldraw::{Vector3, Vector4};

use crate::{
    display_list::InstanceBuffer,
    error::ShaderError,
    part::{EdgeBuffer, MeshBuffer, OptionalEdgeBuffer},
    state::{ProjectionData, ShadingData},
};

#[derive(Debug)]
struct Program<GL: HasContext> {
    gl: Rc<GL>, // This is used only when unallocating

    vertex_shader: GL::Shader,
    fragment_shader: GL::Shader,
    program: GL::Program,
}

impl<GL: HasContext> Program<GL> {
    fn use_program(&self) {
        unsafe {
            self.gl.use_program(Some(self.program));
        }
    }
}

#[derive(Clone)]
struct ShaderSource {
    source: String,
    flags: Vec<(&'static str, Option<String>)>,
}

impl ShaderSource {
    pub fn new(source: String) -> Self {
        ShaderSource {
            source,
            flags: vec![],
        }
    }

    pub fn with_flag(mut self, flag: &'static str) -> Self {
        self.flags.push((flag, None));
        self
    }

    pub fn with_value(mut self, flag: &'static str, value: String) -> Self {
        self.flags.push((flag, Some(value)));
        self
    }

    pub fn build(&self) -> String {
        let mut buf = BufWriter::new(Vec::new());

        if cfg!(target_arch = "wasm32") {
            writeln!(buf, "#version 300 es").unwrap();
        } else {
            writeln!(buf, "#version 330").unwrap();
        }

        for (flag, value) in &self.flags {
            match value {
                Some(v) => {
                    writeln!(buf, "#define {} {}", flag, v).unwrap();
                }
                None => {
                    writeln!(buf, "#define {}", flag).unwrap();
                }
            };
        }

        write!(buf, "{}", self.source).unwrap();

        String::from_utf8(buf.into_inner().unwrap()).unwrap()
    }
}

impl<GL: HasContext> Program<GL> {
    fn compile_shader(gl: &GL, src: &ShaderSource, ty: u32) -> Result<GL::Shader, ShaderError> {
        let shader;

        unsafe {
            shader = match gl.create_shader(ty) {
                Ok(v) => v,
                Err(e) => return Err(ShaderError::CreationError(e)),
            };

            gl.shader_source(shader, &src.build());
            gl.compile_shader(shader);

            if !gl.get_shader_compile_status(shader) {
                println!("{}", src.build());
                Err(ShaderError::CompileError(gl.get_shader_info_log(shader)))
            } else {
                Ok(shader)
            }
        }
    }

    fn compile(
        gl: Rc<GL>,
        vertex_shader: &ShaderSource,
        fragment_shader: &ShaderSource,
    ) -> Result<Program<GL>, ShaderError> {
        let vs = Self::compile_shader(&gl, vertex_shader, glow::VERTEX_SHADER)?;
        let fs = Self::compile_shader(&gl, fragment_shader, glow::FRAGMENT_SHADER)?;

        unsafe {
            let program = match gl.create_program() {
                Ok(v) => v,
                Err(e) => return Err(ShaderError::CreationError(e)),
            };

            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);

            if gl.get_program_link_status(program) {
                Ok(Program {
                    gl: Rc::clone(&gl),
                    vertex_shader: vs,
                    fragment_shader: fs,
                    program,
                })
            } else {
                Err(ShaderError::LinkError(gl.get_program_info_log(program)))
            }
        }
    }
}

impl<GL: HasContext> Drop for Program<GL> {
    fn drop(&mut self) {
        let gl = &self.gl;

        unsafe {
            gl.delete_shader(self.vertex_shader);
            gl.delete_shader(self.fragment_shader);
            gl.delete_program(self.program);
        }
    }
}

pub struct DefaultProgram<GL: HasContext> {
    gl: Rc<GL>,
    program: Program<GL>,

    // Basic projection
    projection: Option<GL::UniformLocation>,
    model_view: Option<GL::UniformLocation>,
    normal_matrix: Option<GL::UniformLocation>,

    // Geometry
    position: Option<u32>,
    normal: Option<u32>,

    // Projection for shading
    view_matrix: Option<GL::UniformLocation>,
    is_orthographic: Option<GL::UniformLocation>,

    // Instancing
    instanced_model_matrix: Option<u32>,

    // Instanced colors
    instanced_color: Option<u32>,

    // Non-instancing
    color: Option<GL::UniformLocation>,

    // Shading
    diffuse: Option<GL::UniformLocation>,
    emissive: Option<GL::UniformLocation>,
    roughness: Option<GL::UniformLocation>,
    metalness: Option<GL::UniformLocation>,
    opacity: Option<GL::UniformLocation>,
    envmap: Option<GL::UniformLocation>,

    local_projection_state: ProjectionData,
    local_shading_state: ShadingData,
}

impl<GL: HasContext> DefaultProgram<GL> {
    fn new(
        gl: Rc<GL>,
        vertex_shader: &ShaderSource,
        fragment_shader: &ShaderSource,
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), vertex_shader, fragment_shader)?;

        let cloned_gl = Rc::clone(&gl);
        let gl: &GL = &gl;

        unsafe {
            Ok(DefaultProgram {
                gl: cloned_gl,

                projection: gl.get_uniform_location(program.program, "projection"),
                model_view: gl.get_uniform_location(program.program, "modelView"),
                normal_matrix: gl.get_uniform_location(program.program, "normalMatrix"),

                position: gl.get_attrib_location(program.program, "position"),
                normal: gl.get_attrib_location(program.program, "normal"),

                view_matrix: gl.get_uniform_location(program.program, "viewMatrix"),
                is_orthographic: gl.get_uniform_location(program.program, "isOrthographic"),

                instanced_model_matrix: gl
                    .get_attrib_location(program.program, "instancedModelMatrix"),

                instanced_color: gl.get_attrib_location(program.program, "instancedColor"),

                color: gl.get_uniform_location(program.program, "color"),

                diffuse: gl.get_uniform_location(program.program, "diffuse"),
                emissive: gl.get_uniform_location(program.program, "emissive"),
                roughness: gl.get_uniform_location(program.program, "roughness"),
                metalness: gl.get_uniform_location(program.program, "metalness"),
                opacity: gl.get_uniform_location(program.program, "opacity"),
                envmap: gl.get_uniform_location(program.program, "envMap"),

                program,

                local_projection_state: ProjectionData::default(),
                local_shading_state: ShadingData {
                    diffuse: Vector3::zero(),
                    emissive: Vector3::zero(),
                    roughness: 0.0,
                    metalness: 0.0,
                    opacity: 0.0,
                },
            })
        }
    }

    fn bind_projection_data(&mut self, projection_data: &ProjectionData) {
        let gl = &self.gl;
        unsafe {
            if projection_data.projection != self.local_projection_state.projection {
                gl.uniform_matrix_4_f32_slice(
                    self.projection.as_ref(),
                    false,
                    AsRef::<[f32; 16]>::as_ref(&projection_data.projection),
                );
                self.local_projection_state.projection = projection_data.projection;
            }
            if projection_data.model_view != self.local_projection_state.model_view {
                gl.uniform_matrix_4_f32_slice(
                    self.model_view.as_ref(),
                    false,
                    AsRef::<[f32; 16]>::as_ref(&projection_data.model_view),
                );
                self.local_projection_state.model_view = projection_data.model_view;
            }
            if projection_data.normal_matrix != self.local_projection_state.normal_matrix {
                gl.uniform_matrix_3_f32_slice(
                    self.normal_matrix.as_ref(),
                    false,
                    AsRef::<[f32; 9]>::as_ref(&projection_data.normal_matrix),
                );
                self.local_projection_state.normal_matrix = projection_data.normal_matrix;
            }
            if projection_data.view_matrix != self.local_projection_state.view_matrix {
                gl.uniform_matrix_4_f32_slice(
                    self.view_matrix.as_ref(),
                    false,
                    AsRef::<[f32; 16]>::as_ref(&projection_data.view_matrix),
                );
                self.local_projection_state.view_matrix = projection_data.view_matrix;
            }
            if projection_data.orthographic != self.local_projection_state.orthographic {
                gl.uniform_1_i32(
                    self.is_orthographic.as_ref(),
                    if projection_data.orthographic { 1 } else { 0 },
                );
                self.local_projection_state.orthographic = projection_data.orthographic;
            }
        }
    }

    fn bind_shading_data(&mut self, shading_data: &ShadingData) {
        let gl = &self.gl;
        unsafe {
            if shading_data.diffuse != self.local_shading_state.diffuse {
                gl.uniform_3_f32_slice(
                    self.diffuse.as_ref(),
                    AsRef::<[f32; 3]>::as_ref(&shading_data.diffuse),
                );
                self.local_shading_state.diffuse = shading_data.diffuse;
            }
            if shading_data.emissive != self.local_shading_state.emissive {
                gl.uniform_3_f32_slice(
                    self.emissive.as_ref(),
                    AsRef::<[f32; 3]>::as_ref(&shading_data.emissive),
                );
                self.local_shading_state.emissive = shading_data.emissive;
            }
            if shading_data.roughness != self.local_shading_state.roughness {
                gl.uniform_1_f32(self.roughness.as_ref(), shading_data.roughness);
                self.local_shading_state.roughness = shading_data.roughness;
            }
            if shading_data.metalness != self.local_shading_state.metalness {
                gl.uniform_1_f32(self.metalness.as_ref(), shading_data.metalness);
                self.local_shading_state.metalness = shading_data.metalness;
            }
            if shading_data.opacity != self.local_shading_state.opacity {
                gl.uniform_1_f32(self.opacity.as_ref(), shading_data.opacity);
                self.local_shading_state.opacity = shading_data.opacity;
            }
        }
    }

    pub fn bind_envmap(&self, texture: &Option<GL::Texture>) {
        let gl = &self.gl;

        self.program.use_program();
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, *texture);
            gl.uniform_1_i32(self.envmap.as_ref(), 0);
        }
    }

    pub fn bind<'a>(
        &'a mut self,
        projection_data: &ProjectionData,
        shading_data: &ShadingData,
    ) -> DefaultProgramBinder<'a, GL> {
        self.program.use_program();
        self.bind_projection_data(projection_data);
        self.bind_shading_data(shading_data);

        DefaultProgramBinder::new(self)
    }
}

pub struct DefaultProgramBinder<'a, GL: HasContext> {
    gl: Rc<GL>,
    program: &'a DefaultProgram<GL>,
}

impl<'a, GL: HasContext> DefaultProgramBinder<'a, GL> {
    fn new(program: &'a DefaultProgram<GL>) -> Self {
        DefaultProgramBinder {
            gl: Rc::clone(&program.gl),
            program,
        }
    }

    pub fn bind_geometry_data(&self, mesh: &MeshBuffer<GL>) -> bool {
        let gl = &self.gl;
        if mesh.buffer_vertices.is_some() && mesh.buffer_normals.is_some() {
            unsafe {
                gl.bind_vertex_array(mesh.array);

                gl.bind_buffer(glow::ARRAY_BUFFER, mesh.buffer_vertices);
                gl.vertex_attrib_pointer_f32(
                    self.program.position.unwrap(),
                    3,
                    glow::FLOAT,
                    false,
                    0,
                    0,
                );
                gl.enable_vertex_attrib_array(self.program.position.unwrap());

                gl.bind_buffer(glow::ARRAY_BUFFER, mesh.buffer_normals);
                gl.vertex_attrib_pointer_f32(
                    self.program.normal.unwrap(),
                    3,
                    glow::FLOAT,
                    false,
                    0,
                    0,
                );
                gl.enable_vertex_attrib_array(self.program.normal.unwrap());
            }
            true
        } else {
            false
        }
    }

    pub fn bind_instanced_geometry_data(&self, instance_buffer: &mut InstanceBuffer<GL>) {
        let gl = &self.gl;

        instance_buffer.update_buffer(gl);
        if self.program.instanced_model_matrix.is_some() {
            let instanced_model_view = self.program.instanced_model_matrix.unwrap();
            unsafe {
                gl.bind_buffer(
                    glow::ARRAY_BUFFER,
                    instance_buffer.model_view_matrices_buffer,
                );
                for i in 0..4 {
                    gl.vertex_attrib_pointer_f32(
                        instanced_model_view + i,
                        4,
                        glow::FLOAT,
                        false,
                        4 * 16,
                        (16 * i) as i32,
                    );
                    gl.enable_vertex_attrib_array(instanced_model_view + i);
                    gl.vertex_attrib_divisor(instanced_model_view + i, 1);
                }
            }
        }
    }

    pub fn bind_non_instanced_color_data(&self, color: &Vector4) {
        let gl = &self.gl;

        unsafe {
            gl.uniform_4_f32_slice(
                self.program.color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&color),
            )
        }
    }

    pub fn bind_instanced_color_data(&self, instance_buffer: &mut InstanceBuffer<GL>) {
        let gl = &self.gl;

        instance_buffer.update_buffer(gl);
        if self.program.instanced_color.is_some() && self.program.instanced_color.is_some() {
            let instanced_color = self.program.instanced_color.unwrap();
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, instance_buffer.color_buffer);
                gl.vertex_attrib_pointer_f32(instanced_color, 4, glow::FLOAT, false, 0, 0);
                gl.enable_vertex_attrib_array(instanced_color);
                gl.vertex_attrib_divisor(instanced_color, 1);
            }
        }
    }
}

impl<'a, GL: HasContext> Drop for DefaultProgramBinder<'a, GL> {
    fn drop(&mut self) {
        let gl = &self.gl;
        if self.program.instanced_model_matrix.is_some() {
            let instanced_model_view = self.program.instanced_model_matrix.unwrap();
            unsafe {
                for i in 0..4 {
                    gl.vertex_attrib_divisor(instanced_model_view + i, 0);
                }
            }
        }
        if let Some(instanced_color) = self.program.instanced_color {
            unsafe {
                gl.vertex_attrib_divisor(instanced_color, 0);
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum DefaultProgramInstancingKind {
    NonInstanced,
    Instanced,
    InstancedWithColors,
}

pub struct EdgeProgram<GL: HasContext> {
    gl: Rc<GL>,
    program: Program<GL>,

    // Basic projection
    projection: Option<GL::UniformLocation>,
    model_view: Option<GL::UniformLocation>,

    // Vertex attributes
    position: Option<u32>,
    color: Option<u32>,

    // Instancing
    instanced_color: Option<u32>,
    instanced_edge_color: Option<u32>,
    instanced_model_matrix: Option<u32>,

    // Non-instancing
    default_color: Option<GL::UniformLocation>,
    edge_color: Option<GL::UniformLocation>,

    local_projection_state: ProjectionData,
}

impl<GL: HasContext> EdgeProgram<GL> {
    fn new(
        gl: Rc<GL>,
        vertex_shader: &ShaderSource,
        fragment_shader: &ShaderSource,
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), vertex_shader, fragment_shader)?;

        let cloned_gl = Rc::clone(&gl);

        unsafe {
            Ok(EdgeProgram {
                gl: cloned_gl,

                projection: gl.get_uniform_location(program.program, "projection"),
                model_view: gl.get_uniform_location(program.program, "modelView"),

                position: gl.get_attrib_location(program.program, "position"),
                color: gl.get_attrib_location(program.program, "color"),

                instanced_color: gl.get_attrib_location(program.program, "instancedColor"),
                instanced_edge_color: gl.get_attrib_location(program.program, "instancedEdgeColor"),
                instanced_model_matrix: gl
                    .get_attrib_location(program.program, "instancedModelMatrix"),

                default_color: gl.get_uniform_location(program.program, "defaultColor"),
                edge_color: gl.get_uniform_location(program.program, "edgeColor"),

                program,

                local_projection_state: ProjectionData::default(),
            })
        }
    }

    fn bind_projection_data(&mut self, projection_data: &ProjectionData) {
        let gl = &self.gl;
        unsafe {
            if projection_data.projection != self.local_projection_state.projection {
                gl.uniform_matrix_4_f32_slice(
                    self.projection.as_ref(),
                    false,
                    AsRef::<[f32; 16]>::as_ref(&projection_data.projection),
                );
                self.local_projection_state.projection = projection_data.projection;
            }
            if projection_data.model_view != self.local_projection_state.model_view {
                gl.uniform_matrix_4_f32_slice(
                    self.model_view.as_ref(),
                    false,
                    AsRef::<[f32; 16]>::as_ref(&projection_data.model_view),
                );
                self.local_projection_state.model_view = projection_data.model_view;
            }
        }
    }

    pub fn bind<'a>(&'a mut self, projection_data: &ProjectionData) -> EdgeProgramBinder<'a, GL> {
        self.program.use_program();
        self.bind_projection_data(projection_data);
        EdgeProgramBinder::new(self)
    }
}

pub struct EdgeProgramBinder<'a, GL: HasContext> {
    gl: Rc<GL>,
    program: &'a EdgeProgram<GL>,
}

impl<'a, GL: HasContext> EdgeProgramBinder<'a, GL> {
    fn new(program: &'a EdgeProgram<GL>) -> Self {
        EdgeProgramBinder {
            gl: Rc::clone(&program.gl),
            program,
        }
    }

    pub fn bind_attribs(&self, edge: &EdgeBuffer<GL>) -> bool {
        let gl = &self.gl;
        if edge.buffer_vertices.is_some() && edge.buffer_colors.is_some() {
            unsafe {
                gl.bind_vertex_array(edge.array);

                gl.bind_buffer(glow::ARRAY_BUFFER, edge.buffer_vertices);
                gl.vertex_attrib_pointer_f32(
                    self.program.position.unwrap(),
                    3,
                    glow::FLOAT,
                    false,
                    0,
                    0,
                );
                gl.enable_vertex_attrib_array(self.program.position.unwrap());

                gl.bind_buffer(glow::ARRAY_BUFFER, edge.buffer_colors);
                gl.vertex_attrib_pointer_f32(
                    self.program.color.unwrap(),
                    3,
                    glow::FLOAT,
                    false,
                    0,
                    0,
                );
                gl.enable_vertex_attrib_array(self.program.color.unwrap());
            }
            true
        } else {
            false
        }
    }

    pub fn bind_instanced_attribs(&self, instance_buffer: &mut InstanceBuffer<GL>) {
        let gl = &self.gl;

        instance_buffer.update_buffer(gl);
        if self.program.instanced_model_matrix.is_some() {
            let instanced_model_view = self.program.instanced_model_matrix.unwrap();
            unsafe {
                gl.bind_buffer(
                    glow::ARRAY_BUFFER,
                    instance_buffer.model_view_matrices_buffer,
                );
                for i in 0..4 {
                    gl.vertex_attrib_pointer_f32(
                        instanced_model_view + i,
                        4,
                        glow::FLOAT,
                        false,
                        4 * 16,
                        (16 * i) as i32,
                    );
                    gl.enable_vertex_attrib_array(instanced_model_view + i);
                    gl.vertex_attrib_divisor(instanced_model_view + i, 1);
                }
            }
        }
        if let Some(instanced_color) = self.program.instanced_color {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, instance_buffer.color_buffer);
                gl.vertex_attrib_pointer_f32(instanced_color, 4, glow::FLOAT, false, 0, 0);
                gl.enable_vertex_attrib_array(instanced_color);
                gl.vertex_attrib_divisor(instanced_color, 1);
            }
        }
        if let Some(instanced_edge_color) = self.program.instanced_edge_color {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, instance_buffer.edge_color_buffer);
                gl.vertex_attrib_pointer_f32(instanced_edge_color, 4, glow::FLOAT, false, 0, 0);
                gl.enable_vertex_attrib_array(instanced_edge_color);
                gl.vertex_attrib_divisor(instanced_edge_color, 1);
            }
        }
    }

    pub fn bind_non_instanced_properties(&self, color: &Vector4, edge_color: &Vector4) {
        let gl = &self.gl;
        unsafe {
            gl.uniform_4_f32_slice(
                self.program.default_color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&color),
            );
            gl.uniform_4_f32_slice(
                self.program.edge_color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&edge_color),
            );
        }
    }
}

impl<'a, GL: HasContext> Drop for EdgeProgramBinder<'a, GL> {
    fn drop(&mut self) {
        let gl = &self.gl;
        if self.program.instanced_model_matrix.is_some() {
            let instanced_model_view = self.program.instanced_model_matrix.unwrap();
            unsafe {
                for i in 0..4 {
                    gl.vertex_attrib_divisor(instanced_model_view + i, 0);
                }
            }
        }
        if let Some(instanced_color) = self.program.instanced_color {
            unsafe {
                gl.vertex_attrib_divisor(instanced_color, 0);
            }
        }
        if let Some(instanced_edge_color) = self.program.instanced_edge_color {
            unsafe {
                gl.vertex_attrib_divisor(instanced_edge_color, 0);
            }
        }
    }
}

pub struct OptionalEdgeProgram<GL: HasContext> {
    gl: Rc<GL>,
    program: Program<GL>,

    // Basic projection
    projection: Option<GL::UniformLocation>,
    model_view: Option<GL::UniformLocation>,

    // Vertex attributes
    position: Option<u32>,
    color: Option<u32>,
    control1: Option<u32>,
    control2: Option<u32>,
    direction: Option<u32>,

    // Instancing
    instanced_color: Option<u32>,
    instanced_edge_color: Option<u32>,
    instanced_model_matrix: Option<u32>,

    // Non-instancing
    default_color: Option<GL::UniformLocation>,
    edge_color: Option<GL::UniformLocation>,

    local_projection_state: ProjectionData,
}

impl<GL: HasContext> OptionalEdgeProgram<GL> {
    fn new(
        gl: Rc<GL>,
        vertex_shader: &ShaderSource,
        fragment_shader: &ShaderSource,
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), vertex_shader, fragment_shader)?;

        let cloned_gl = Rc::clone(&gl);
        let gl: &GL = &gl;

        unsafe {
            Ok(OptionalEdgeProgram {
                gl: cloned_gl,

                projection: gl.get_uniform_location(program.program, "projection"),
                model_view: gl.get_uniform_location(program.program, "modelView"),

                position: gl.get_attrib_location(program.program, "position"),
                color: gl.get_attrib_location(program.program, "color"),
                control1: gl.get_attrib_location(program.program, "control1"),
                control2: gl.get_attrib_location(program.program, "control2"),
                direction: gl.get_attrib_location(program.program, "direction"),

                instanced_color: gl.get_attrib_location(program.program, "instancedColor"),
                instanced_edge_color: gl.get_attrib_location(program.program, "instancedEdgeColor"),
                instanced_model_matrix: gl
                    .get_attrib_location(program.program, "instancedModelMatrix"),

                default_color: gl.get_uniform_location(program.program, "defaultColor"),
                edge_color: gl.get_uniform_location(program.program, "edgeColor"),

                program,

                local_projection_state: ProjectionData::default(),
            })
        }
    }

    fn bind_projection_data(&mut self, projection_data: &ProjectionData) {
        let gl = &self.gl;
        unsafe {
            if projection_data.projection != self.local_projection_state.projection {
                gl.uniform_matrix_4_f32_slice(
                    self.projection.as_ref(),
                    false,
                    AsRef::<[f32; 16]>::as_ref(&projection_data.projection),
                );
                self.local_projection_state.projection = projection_data.projection;
            }
            if projection_data.model_view != self.local_projection_state.model_view {
                gl.uniform_matrix_4_f32_slice(
                    self.model_view.as_ref(),
                    false,
                    AsRef::<[f32; 16]>::as_ref(&projection_data.model_view),
                );
                self.local_projection_state.model_view = projection_data.model_view;
            }
        }
    }

    pub fn bind<'a>(
        &'a mut self,
        projection_data: &ProjectionData,
    ) -> OptionalEdgeProgramBinder<'a, GL> {
        self.program.use_program();
        self.bind_projection_data(projection_data);

        OptionalEdgeProgramBinder::new(self)
    }
}

pub struct OptionalEdgeProgramBinder<'a, GL: HasContext> {
    gl: Rc<GL>,
    program: &'a OptionalEdgeProgram<GL>,
}

impl<'a, GL: HasContext> OptionalEdgeProgramBinder<'a, GL> {
    fn new(program: &'a OptionalEdgeProgram<GL>) -> Self {
        program.program.use_program();

        OptionalEdgeProgramBinder {
            gl: Rc::clone(&program.gl),
            program,
        }
    }

    pub fn bind_attribs(&self, optional_edge: &OptionalEdgeBuffer<GL>) {
        let gl = &self.gl;
        unsafe {
            gl.bind_vertex_array(optional_edge.array);

            gl.bind_buffer(glow::ARRAY_BUFFER, optional_edge.buffer_vertices);
            gl.vertex_attrib_pointer_f32(
                self.program.position.unwrap(),
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.enable_vertex_attrib_array(self.program.position.unwrap());

            gl.bind_buffer(glow::ARRAY_BUFFER, optional_edge.buffer_colors);
            gl.vertex_attrib_pointer_f32(self.program.color.unwrap(), 3, glow::FLOAT, false, 0, 0);
            gl.enable_vertex_attrib_array(self.program.color.unwrap());

            gl.bind_buffer(glow::ARRAY_BUFFER, optional_edge.buffer_controls_1);
            gl.vertex_attrib_pointer_f32(
                self.program.control1.unwrap(),
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.enable_vertex_attrib_array(self.program.control1.unwrap());

            gl.bind_buffer(glow::ARRAY_BUFFER, optional_edge.buffer_controls_2);
            gl.vertex_attrib_pointer_f32(
                self.program.control2.unwrap(),
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.enable_vertex_attrib_array(self.program.control2.unwrap());

            gl.bind_buffer(glow::ARRAY_BUFFER, optional_edge.buffer_directions);
            gl.vertex_attrib_pointer_f32(
                self.program.direction.unwrap(),
                3,
                glow::FLOAT,
                false,
                0,
                0,
            );
            gl.enable_vertex_attrib_array(self.program.direction.unwrap());
        }
    }

    pub fn bind_instanced_attribs(&self, instance_buffer: &mut InstanceBuffer<GL>) {
        let gl = &self.gl;

        instance_buffer.update_buffer(gl);
        if self.program.instanced_model_matrix.is_some() {
            let instanced_model_view = self.program.instanced_model_matrix.unwrap();
            unsafe {
                gl.bind_buffer(
                    glow::ARRAY_BUFFER,
                    instance_buffer.model_view_matrices_buffer,
                );
                for i in 0..4 {
                    gl.vertex_attrib_pointer_f32(
                        instanced_model_view + i,
                        4,
                        glow::FLOAT,
                        false,
                        4 * 16,
                        (16 * i) as i32,
                    );
                    gl.enable_vertex_attrib_array(instanced_model_view + i);
                    gl.vertex_attrib_divisor(instanced_model_view + i, 1);
                }
            }
        }
        if let Some(instanced_color) = self.program.instanced_color {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, instance_buffer.color_buffer);
                gl.vertex_attrib_pointer_f32(instanced_color, 4, glow::FLOAT, false, 0, 0);
                gl.enable_vertex_attrib_array(instanced_color);
                gl.vertex_attrib_divisor(instanced_color, 1);
            }
        }
        if let Some(instanced_edge_color) = self.program.instanced_edge_color {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, instance_buffer.edge_color_buffer);
                gl.vertex_attrib_pointer_f32(instanced_edge_color, 4, glow::FLOAT, false, 0, 0);
                gl.enable_vertex_attrib_array(instanced_edge_color);
                gl.vertex_attrib_divisor(instanced_edge_color, 1);
            }
        }
    }

    pub fn bind_non_instanced_properties(&self, color: &Vector4, edge_color: &Vector4) {
        let gl = &self.gl;
        unsafe {
            gl.uniform_4_f32_slice(
                self.program.default_color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&color),
            );
            gl.uniform_4_f32_slice(
                self.program.edge_color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&edge_color),
            );
        }
    }
}

impl<'a, GL: HasContext> Drop for OptionalEdgeProgramBinder<'a, GL> {
    fn drop(&mut self) {
        let gl = &self.gl;
        if self.program.instanced_model_matrix.is_some() {
            let instanced_model_view = self.program.instanced_model_matrix.unwrap();
            unsafe {
                for i in 0..4 {
                    gl.vertex_attrib_divisor(instanced_model_view + i, 0);
                }
            }
        }
        if let Some(instanced_color) = self.program.instanced_color {
            unsafe {
                gl.vertex_attrib_divisor(instanced_color, 0);
            }
        }
        if let Some(instanced_edge_color) = self.program.instanced_edge_color {
            unsafe {
                gl.vertex_attrib_divisor(instanced_edge_color, 0);
            }
        }
    }
}

pub struct ProgramManager<GL: HasContext> {
    pub default: DefaultProgram<GL>,
    pub default_instanced: DefaultProgram<GL>,
    pub default_instanced_with_colors: DefaultProgram<GL>,

    pub default_without_bfc: DefaultProgram<GL>,
    pub default_without_bfc_instanced: DefaultProgram<GL>,
    pub default_without_bfc_instanced_with_colors: DefaultProgram<GL>,

    pub edge: EdgeProgram<GL>,
    pub edge_instanced: EdgeProgram<GL>,

    pub optional_edge: OptionalEdgeProgram<GL>,
    pub optional_edge_instanced: OptionalEdgeProgram<GL>,
}

impl<GL: HasContext> ProgramManager<GL> {
    pub fn new(gl: Rc<GL>) -> Result<ProgramManager<GL>, ShaderError> {
        let default_fs = ShaderSource::new(
            String::from_utf8(include_bytes!("../shaders/default.fs").to_vec()).unwrap(),
        );
        let default_vs = ShaderSource::new(
            String::from_utf8(include_bytes!("../shaders/default.vs").to_vec()).unwrap(),
        );

        let default = DefaultProgram::new(Rc::clone(&gl), &default_vs, &default_fs)?;
        let default_instanced = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs.clone().with_flag("USE_INSTANCING"),
            &default_fs.clone().with_flag("USE_INSTANCING"),
        )?;
        let default_instanced_with_colors = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs
                .clone()
                .with_flag("USE_INSTANCING")
                .with_flag("USE_INSTANCED_COLORS"),
            &default_fs
                .clone()
                .with_flag("USE_INSTANCING")
                .with_flag("USE_INSTANCED_COLORS"),
        )?;
        let default_without_bfc = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs.clone().with_flag("WITHOUT_BFC"),
            &default_fs.clone().with_flag("WITHOUT_BFC"),
        )?;
        let default_without_bfc_instanced = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs
                .clone()
                .with_flag("WITHOUT_BFC")
                .with_flag("USE_INSTANCING"),
            &default_fs
                .clone()
                .with_flag("WITHOUT_BFC")
                .with_flag("USE_INSTANCING"),
        )?;
        let default_without_bfc_instanced_with_colors = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs
                .with_flag("WITHOUT_BFC")
                .with_flag("USE_INSTANCING")
                .with_flag("USE_INSTANCED_COLORS"),
            &default_fs
                .with_flag("WITHOUT_BFC")
                .with_flag("USE_INSTANCING")
                .with_flag("USE_INSTANCED_COLORS"),
        )?;

        let edge_fs = ShaderSource::new(
            String::from_utf8(include_bytes!("../shaders/edge.fs").to_vec()).unwrap(),
        );
        let edge_vs = ShaderSource::new(
            String::from_utf8(include_bytes!("../shaders/edge.vs").to_vec()).unwrap(),
        );

        let edge = EdgeProgram::new(Rc::clone(&gl), &edge_vs, &edge_fs)?;
        let edge_instanced = EdgeProgram::new(
            Rc::clone(&gl),
            &edge_vs.with_flag("USE_INSTANCING"),
            &edge_fs.with_flag("USE_INSTANCING"),
        )?;

        let optional_edge_fs = ShaderSource::new(
            String::from_utf8(include_bytes!("../shaders/optional_edge.fs").to_vec()).unwrap(),
        );
        let optional_edge_vs = ShaderSource::new(
            String::from_utf8(include_bytes!("../shaders/optional_edge.vs").to_vec()).unwrap(),
        );

        let optional_edge =
            OptionalEdgeProgram::new(Rc::clone(&gl), &optional_edge_vs, &optional_edge_fs)?;
        let optional_edge_instanced = OptionalEdgeProgram::new(
            Rc::clone(&gl),
            &optional_edge_vs.with_flag("USE_INSTANCING"),
            &optional_edge_fs.with_flag("USE_INSTANCING"),
        )?;

        Ok(ProgramManager {
            default,
            default_instanced,
            default_instanced_with_colors,
            default_without_bfc,
            default_without_bfc_instanced,
            default_without_bfc_instanced_with_colors,

            edge,
            edge_instanced,

            optional_edge,
            optional_edge_instanced,
        })
    }

    pub fn get_default_program(
        &mut self,
        instancing_kind: DefaultProgramInstancingKind,
        bfc: bool,
    ) -> &mut DefaultProgram<GL> {
        match (instancing_kind, bfc) {
            (DefaultProgramInstancingKind::NonInstanced, true) => &mut self.default,
            (DefaultProgramInstancingKind::Instanced, true) => &mut self.default_instanced,
            (DefaultProgramInstancingKind::InstancedWithColors, true) => {
                &mut self.default_instanced_with_colors
            }
            (DefaultProgramInstancingKind::NonInstanced, false) => &mut self.default_without_bfc,
            (DefaultProgramInstancingKind::Instanced, false) => {
                &mut self.default_without_bfc_instanced
            }
            (DefaultProgramInstancingKind::InstancedWithColors, false) => {
                &mut self.default_without_bfc_instanced_with_colors
            }
        }
    }

    pub fn get_edge_program(&mut self, instanced: bool) -> &mut EdgeProgram<GL> {
        if instanced {
            &mut self.edge_instanced
        } else {
            &mut self.edge
        }
    }

    pub fn get_optional_edge_program(&mut self, instanced: bool) -> &mut OptionalEdgeProgram<GL> {
        if instanced {
            &mut self.optional_edge_instanced
        } else {
            &mut self.optional_edge
        }
    }

    pub fn bind_envmap(&self, texture: &Option<GL::Texture>) {
        self.default.bind_envmap(texture);
        self.default_instanced.bind_envmap(texture);
        self.default_instanced_with_colors.bind_envmap(texture);
    }
}
