use std::rc::Rc;
use std::str;

use glow::HasContext;
use ldraw::{Matrix3, Matrix4, Vector4};

use crate::{
    error::ShaderError,
    state::{ProjectionData, ShadingData},
};

#[derive(Debug)]
struct Program<GL: HasContext> {
    gl: Rc<GL>, // This is used only when unallocating

    vertex_shader: GL::Shader,
    fragment_shader: GL::Shader,
    program: GL::Program,
}

fn borrow_uniform_location<GL: HasContext>(e: &Option<GL::UniformLocation>) -> Option<&GL::UniformLocation> {
    match e {
        Some(e) => Some(&e),
        None => None,
    }
}

pub trait Bindable {
    fn bind(&self) -> &Self;
    fn unbind(&self);
}

impl<GL: HasContext> Program<GL> {
    fn compile_shader(gl: &GL, src: &str, ty: u32) -> Result<GL::Shader, ShaderError> {
        let shader;

        unsafe {
            shader = match gl.create_shader(ty) {
                Ok(v) => v,
                Err(e) => return Err(ShaderError::CreationError(e)),
            };

            gl.shader_source(shader, src);
            gl.compile_shader(shader);

            if !gl.get_shader_compile_status(shader) {
                Err(ShaderError::CompileError(gl.get_shader_info_log(shader)))
            } else {
                Ok(shader)
            }
        }
    }

    pub fn compile(
        gl: Rc<GL>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Program<GL>, ShaderError> {
        let gl_ = &gl;

        let vs = Self::compile_shader(gl_, &vertex_shader, glow::VERTEX_SHADER)?;
        let fs = Self::compile_shader(gl_, &fragment_shader, glow::FRAGMENT_SHADER)?;

        unsafe {
            let program = match gl_.create_program() {
                Ok(v) => v,
                Err(e) => return Err(ShaderError::CreationError(e)),
            };

            gl_.attach_shader(program, vs);
            gl_.attach_shader(program, fs);
            gl_.link_program(program);

            if gl_.get_program_link_status(program) {
                Ok(Program {
                    gl: Rc::clone(&gl),
                    vertex_shader: vs,
                    fragment_shader: fs,
                    program,
                })
            } else {
                Err(ShaderError::LinkError(gl_.get_program_info_log(program)))
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

struct ProjectionUniforms<GL: HasContext> {
    projection: Option<GL::UniformLocation>,
    model_view: Option<GL::UniformLocation>,
    view_matrix: Option<GL::UniformLocation>,
    normal_matrix: Option<GL::UniformLocation>,
}

impl<GL: HasContext> ProjectionUniforms<GL> {
    pub fn new(gl: &GL, program: &Program<GL>) -> ProjectionUniforms<GL> {
        unsafe {
            ProjectionUniforms {
                projection: gl.get_uniform_location(program.program, "projection"),
                model_view: gl.get_uniform_location(program.program, "modelView"),
                view_matrix: gl.get_uniform_location(program.program, "viewMatrix"),
                normal_matrix: gl.get_uniform_location(program.program, "normalMatrix"),
            }
        }
    }

    pub fn bind(&self, gl: &GL, projection_params: &ProjectionData, normal_matrix: Option<&[f32; 9]>) {
        unsafe {
            gl.uniform_matrix_4_f32_slice(
                borrow_uniform_location::<GL>(&self.projection),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_params.projection)
            );
            gl.uniform_matrix_4_f32_slice(
                borrow_uniform_location::<GL>(&self.model_view),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_params.model_view.last().unwrap())
            );
            gl.uniform_matrix_4_f32_slice(
                borrow_uniform_location::<GL>(&self.view_matrix),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_params.view_matrix)
            );
            if let Some(e) = normal_matrix {
                gl.uniform_matrix_3_f32_slice(
                    borrow_uniform_location::<GL>(&self.normal_matrix),
                    false,
                    e
                );
            }
        }
    }
}

pub struct ShadedProgram<GL: HasContext> {
    program: Program<GL>,

    projection_uniforms: ProjectionUniforms<GL>,
    uniform_color: Option<GL::UniformLocation>,
    uniform_light_color: Option<GL::UniformLocation>,
    uniform_light_direction: Option<GL::UniformLocation>,

    pub attrib_position: Option<u32>,
    pub attrib_normal: Option<u32>,
}

impl<GL: HasContext> ShadedProgram<GL> {
    pub fn new(
        gl: Rc<GL>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), &vertex_shader, &fragment_shader)?;

        let gl = &gl;
        let projection_uniforms: ProjectionUniforms<GL> = ProjectionUniforms::new(&gl, &program);
        unsafe {
            let uniform_color = gl.get_uniform_location(program.program, "color");
            let uniform_light_color = gl.get_uniform_location(program.program, "lightColor");
            let uniform_light_direction =
                gl.get_uniform_location(program.program, "lightDirection");
            let attrib_position = gl.get_attrib_location(program.program, "position");
            let attrib_normal = gl.get_attrib_location(program.program, "normal");

            Ok(Self {
                program,
                projection_uniforms,
                uniform_color,
                uniform_light_color,
                uniform_light_direction,
                attrib_position,
                attrib_normal,
            })
        }
    }

    pub fn bind_uniforms(
        &self,
        projection_params: &ProjectionData,
        normal_matrix: &[f32; 9],
        shading_params: &ShadingData,
        color: &[f32; 4],
    ) {
        let gl = &self.program.gl;
        self.projection_uniforms.bind(&gl, &projection_params, Some(&normal_matrix));
        unsafe {
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<GL>(&self.uniform_color),
                color
            );
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<GL>(&self.uniform_light_color),
                AsRef::<[f32; 4]>::as_ref(&shading_params.light_color)
            );
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<GL>(&self.uniform_light_direction),
                AsRef::<[f32; 4]>::as_ref(&shading_params.light_direction)
            );
        }
    }
}

impl<GL: HasContext> Bindable for ShadedProgram<GL> {
    fn bind(&self) -> &Self {
        let gl = &self.program.gl;
        unsafe {
            gl.use_program(Some(self.program.program));
            if let Some(e) = self.attrib_position {
                gl.enable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_normal {
                gl.enable_vertex_attrib_array(e);
            }
        }
        self
    }

    fn unbind(&self) {
        let gl = &self.program.gl;
        unsafe {
            if let Some(e) = self.attrib_position {
                gl.disable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_normal {
                gl.disable_vertex_attrib_array(e);
            }
        }
    }
}

pub struct InstancedShadedProgram<GL: HasContext> {
    program: Program<GL>,

    projection_uniforms: ProjectionUniforms<GL>,
    uniform_light_color: Option<GL::UniformLocation>,
    uniform_light_direction: Option<GL::UniformLocation>,

    pub attrib_position: Option<u32>,
    pub attrib_normal: Option<u32>,
    pub attrib_instanced_model_view: Option<u32>,
    pub attrib_instanced_normal_matrix: Option<u32>,
    pub attrib_instanced_color: Option<u32>,
}

impl<GL: HasContext> InstancedShadedProgram<GL> {
    pub fn new(
        gl: Rc<GL>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), &vertex_shader, &fragment_shader)?;

        let gl = &gl;
        let projection_uniforms: ProjectionUniforms<GL> = ProjectionUniforms::new(&gl, &program);
        unsafe {
            let uniform_light_color = gl.get_uniform_location(program.program, "lightColor");
            let uniform_light_direction =
                gl.get_uniform_location(program.program, "lightDirection");
            let attrib_position = gl.get_attrib_location(program.program, "position");
            let attrib_normal = gl.get_attrib_location(program.program, "normal");
            let attrib_instanced_model_view = gl.get_attrib_location(program.program, "instancedModelView");
            let attrib_instanced_normal_matrix = gl.get_attrib_location(program.program, "instancedNormalMatrix");
            let attrib_instanced_color = gl.get_attrib_location(program.program, "instancedColor");

            Ok(Self {
                program,
                projection_uniforms,
                uniform_light_color,
                uniform_light_direction,
                attrib_position,
                attrib_normal,
                attrib_instanced_model_view,
                attrib_instanced_normal_matrix,
                attrib_instanced_color,
            })
        }
    }

    pub fn bind_uniforms(
        &self,
        projection_params: &ProjectionData,
        shading_params: &ShadingData,
    ) {
        let gl = &self.program.gl;
        self.projection_uniforms.bind(&gl, &projection_params, None);
        unsafe {
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<GL>(&self.uniform_light_color),
                AsRef::<[f32; 4]>::as_ref(&shading_params.light_color)
            );
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<GL>(&self.uniform_light_direction),
                AsRef::<[f32; 4]>::as_ref(&shading_params.light_direction)
            );
        }
    }
}

impl<GL: HasContext> Bindable for InstancedShadedProgram<GL> {
    fn bind(&self) -> &Self {
        let gl = &self.program.gl;
        unsafe {
            gl.use_program(Some(self.program.program));
            if let Some(e) = self.attrib_position {
                gl.enable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_normal {
                gl.enable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_instanced_model_view {
                gl.enable_vertex_attrib_array(e);
                gl.enable_vertex_attrib_array(e + 1);
                gl.enable_vertex_attrib_array(e + 2);
                gl.enable_vertex_attrib_array(e + 3);
            }
            if let Some(e) = self.attrib_instanced_normal_matrix {
                gl.enable_vertex_attrib_array(e);
                gl.enable_vertex_attrib_array(e + 1);
                gl.enable_vertex_attrib_array(e + 2);
            }
            if let Some(e) = self.attrib_instanced_color {
                gl.enable_vertex_attrib_array(e);
            }
        }
        self
    }

    fn unbind(&self) {
        let gl = &self.program.gl;
        unsafe {
            if let Some(e) = self.attrib_position {
                gl.disable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_normal {
                gl.disable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_instanced_model_view {
                gl.disable_vertex_attrib_array(e);
                gl.disable_vertex_attrib_array(e + 1);
                gl.disable_vertex_attrib_array(e + 2);
                gl.disable_vertex_attrib_array(e + 3);
            }
            if let Some(e) = self.attrib_instanced_normal_matrix {
                gl.disable_vertex_attrib_array(e);
                gl.disable_vertex_attrib_array(e + 1);
                gl.disable_vertex_attrib_array(e + 2);
            }
            if let Some(e) = self.attrib_instanced_color {
                gl.disable_vertex_attrib_array(e);
            }
        }
    }
}

pub struct EdgeProgram<GL: HasContext> {
    program: Program<GL>,

    projection_uniforms: ProjectionUniforms<GL>,

    pub uniform_color_default: Option<GL::UniformLocation>,
    pub uniform_color_edge: Option<GL::UniformLocation>,

    pub attrib_position: Option<u32>,
    pub attrib_colors: Option<u32>,
}

impl<GL: HasContext> EdgeProgram<GL> {
    pub fn new(
        gl: Rc<GL>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), &vertex_shader, &fragment_shader)?;
        let gl = &gl;
        let projection_uniforms: ProjectionUniforms<GL> = ProjectionUniforms::new(&gl, &program);
        let uniform_color_default = unsafe { gl.get_uniform_location(program.program, "colorDefault") };
        let uniform_color_edge = unsafe { gl.get_uniform_location(program.program, "colorEdge") };
        let attrib_position = unsafe { gl.get_attrib_location(program.program, "position") };
        let attrib_colors = unsafe { gl.get_attrib_location(program.program, "color") };
        Ok(Self {
            program,
            projection_uniforms,
            uniform_color_default,
            uniform_color_edge,
            attrib_position,
            attrib_colors,
        })
    }

    pub fn bind_uniforms(&self, projection_params: &ProjectionData) {
        let gl = &self.program.gl;
        self.projection_uniforms.bind(&gl, &projection_params, None);
    }
}

impl<GL: HasContext> Bindable for EdgeProgram<GL> {
    fn bind(&self) -> &Self {
        let gl = &self.program.gl;
        unsafe {
            gl.use_program(Some(self.program.program));
            if let Some(e) = self.attrib_position {
                gl.enable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_colors {
                gl.enable_vertex_attrib_array(e);
            }
        }
        self
    }

    fn unbind(&self) {
        let gl = &self.program.gl;
        unsafe {
            if let Some(e) = self.attrib_position {
                gl.disable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_colors {
                gl.disable_vertex_attrib_array(e);
            }
        }
    }
}

pub struct InstancedEdgeProgram<GL: HasContext> {
    program: Program<GL>,

    projection_uniforms: ProjectionUniforms<GL>,

    pub attrib_position: Option<u32>,
    pub attrib_colors: Option<u32>,
    pub attrib_instanced_color_default: Option<u32>,
    pub attrib_instanced_color_edge: Option<u32>,
    pub attrib_instanced_model_view: Option<u32>,
}

impl<GL: HasContext> InstancedEdgeProgram<GL> {
    pub fn new(
        gl: Rc<GL>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), &vertex_shader, &fragment_shader)?;
        let gl = &gl;
        let projection_uniforms: ProjectionUniforms<GL> = ProjectionUniforms::new(&gl, &program);
        let attrib_position = unsafe { gl.get_attrib_location(program.program, "position") };
        let attrib_colors = unsafe { gl.get_attrib_location(program.program, "color") };
        let attrib_instanced_color_default = unsafe { gl.get_attrib_location(program.program, "instancedColorDefault") };
        let attrib_instanced_color_edge = unsafe { gl.get_attrib_location(program.program, "instancedColorEdge") };
        let attrib_instanced_model_view = unsafe { gl.get_attrib_location(program.program, "instancedModelView") };
        Ok(Self {
            program,
            projection_uniforms,
            attrib_position,
            attrib_colors,
            attrib_instanced_color_default,
            attrib_instanced_color_edge,
            attrib_instanced_model_view,
        })
    }

    pub fn bind_uniforms(&self, projection_params: &ProjectionData) {
        let gl = &self.program.gl;
        self.projection_uniforms.bind(&gl, &projection_params, None);
    }
}

impl<GL: HasContext> Bindable for InstancedEdgeProgram<GL> {
    fn bind(&self) -> &Self {
        let gl = &self.program.gl;
        unsafe {
            gl.use_program(Some(self.program.program));
            if let Some(e) = self.attrib_position {
                gl.enable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_colors {
                gl.enable_vertex_attrib_array(e);
            }
        }
        self
    }

    fn unbind(&self) {
        let gl = &self.program.gl;
        unsafe {
            if let Some(e) = self.attrib_position {
                gl.disable_vertex_attrib_array(e);
            }
            if let Some(e) = self.attrib_colors {
                gl.disable_vertex_attrib_array(e);
            }
        }
    }
}

pub enum ProgramKind<'a, GL: HasContext> {
    Solid(&'a ShadedProgram<GL>),
    SolidFlat(&'a ShadedProgram<GL>),
    Edge(&'a EdgeProgram<GL>),
    InstancedSolid(&'a InstancedShadedProgram<GL>),
    InstancedSolidFlat(&'a InstancedShadedProgram<GL>),
    InstancedEdge(&'a InstancedEdgeProgram<GL>),
}

impl<'a, GL: HasContext> ProgramKind<'a, GL> {
    pub fn unbind(&self) {
        match self {
            Self::Solid(e) | Self::SolidFlat(e) => e.unbind(),
            Self::Edge(e) => e.unbind(),
            Self::InstancedSolid(e) | Self::InstancedSolidFlat(e) => e.unbind(),
            Self::InstancedEdge(e) => e.unbind(),
        };
    }
}

pub struct ProgramManager<GL: HasContext> {
    pub solid: ShadedProgram<GL>,
    pub solid_flat: ShadedProgram<GL>,
    pub edge: EdgeProgram<GL>,

    pub instanced_solid: InstancedShadedProgram<GL>,
    pub instanced_solid_flat: InstancedShadedProgram<GL>,
    pub instanced_edge: InstancedEdgeProgram<GL>,
}

impl<GL: HasContext> ProgramManager<GL> {
    pub fn new(gl: Rc<GL>) -> Result<ProgramManager<GL>, ShaderError> {
        let solid_fs = str::from_utf8(include_bytes!("../shaders/default.fs")).unwrap();
        let solid_vs = str::from_utf8(include_bytes!("../shaders/default.vs")).unwrap();
        let solid_fs_with_bfc = solid_fs.replace("##IS_BFC_CERTIFIED##", "true");
        let solid_fs_without_bfc = solid_fs.replace("##IS_BFC_CERTIFIED##", "false");
        let solid = ShadedProgram::new(Rc::clone(&gl), &solid_vs, &solid_fs_with_bfc)?;
        let solid_flat = ShadedProgram::new(Rc::clone(&gl), &solid_vs, &solid_fs_without_bfc)?;

        let instanced_solid_fs = str::from_utf8(include_bytes!("../shaders/default_instanced.fs")).unwrap();
        let instanced_solid_vs = str::from_utf8(include_bytes!("../shaders/default_instanced.vs")).unwrap();
        let instanced_solid_fs_with_bfc = instanced_solid_fs.replace("##IS_BFC_CERTIFIED##", "true");
        let instanced_solid_fs_without_bfc = instanced_solid_fs.replace("##IS_BFC_CERTIFIED##", "false");
        let instanced_solid = InstancedShadedProgram::new(Rc::clone(&gl), &instanced_solid_vs, &instanced_solid_fs_with_bfc)?;
        let instanced_solid_flat = InstancedShadedProgram::new(Rc::clone(&gl), &instanced_solid_vs, &instanced_solid_fs_without_bfc)?;

        let edge_fs = str::from_utf8(include_bytes!("../shaders/edge.fs")).unwrap();
        let edge_vs = str::from_utf8(include_bytes!("../shaders/edge.vs")).unwrap();
        let edge = EdgeProgram::new(Rc::clone(&gl), &edge_vs, &edge_fs)?;
        
        let instanced_edge_fs = str::from_utf8(include_bytes!("../shaders/edge_instanced.fs")).unwrap();
        let instanced_edge_vs = str::from_utf8(include_bytes!("../shaders/edge_instanced.vs")).unwrap();
        let instanced_edge = InstancedEdgeProgram::new(Rc::clone(&gl), &instanced_edge_vs, &instanced_edge_fs)?;

        Ok(ProgramManager {
            solid,
            solid_flat,
            edge,

            instanced_solid,
            instanced_solid_flat,
            instanced_edge
        })
    }
}

