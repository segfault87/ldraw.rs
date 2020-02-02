use std::rc::Rc;
use std::str;

use ldraw::Vector4;

use crate::error::ShaderError;
use crate::scene::{ProjectionParams, ShadingParams};
use crate::GL;

#[derive(Debug)]
struct Program<T: GL> {
    gl: Rc<T>, // This is used only when unallocating

    vertex_shader: T::Shader,
    fragment_shader: T::Shader,
    program: T::Program,
}

fn borrow_uniform_location<T: GL>(e: &Option<T::UniformLocation>) -> Option<&T::UniformLocation> {
    match e {
        Some(e) => Some(&e),
        None => None,
    }
}

pub trait Bindable {
    fn bind(&self) -> &Self;
    fn unbind(&self);
}

impl<T: GL> Program<T> {
    fn compile_shader(gl: &T, src: &str, ty: u32) -> Result<T::Shader, ShaderError> {
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
        gl: Rc<T>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Program<T>, ShaderError> {
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

impl<T: GL> Drop for Program<T> {
    fn drop(&mut self) {
        let gl = &self.gl;

        unsafe {
            gl.delete_shader(self.vertex_shader);
            gl.delete_shader(self.fragment_shader);
            gl.delete_program(self.program);
        }
    }
}

struct ProjectionUniforms<T: GL> {
    projection: Option<T::UniformLocation>,
    model_view: Option<T::UniformLocation>,
    view_matrix: Option<T::UniformLocation>,
}

impl<T: GL> ProjectionUniforms<T> {
    pub fn new(gl: &T, program: &Program<T>) -> ProjectionUniforms<T> {
        unsafe {
            ProjectionUniforms {
                projection: gl.get_uniform_location(program.program, "projection"),
                model_view: gl.get_uniform_location(program.program, "modelView"),
                view_matrix: gl.get_uniform_location(program.program, "viewMatrix"),
            }
        }
    }

    pub fn bind(&self, gl: &T, projection_params: &ProjectionParams) {
        unsafe {
            gl.uniform_matrix_4_f32_slice(
                borrow_uniform_location::<T>(&self.projection),
                false,
                projection_params.projection.as_ref(),
            );
            gl.uniform_matrix_4_f32_slice(
                borrow_uniform_location::<T>(&self.model_view),
                false,
                projection_params.model_view.as_ref(),
            );
            gl.uniform_matrix_4_f32_slice(
                borrow_uniform_location::<T>(&self.view_matrix),
                false,
                projection_params.view_matrix.as_ref(),
            );
        }
    }
}

pub struct ShadedProgram<T: GL> {
    program: Program<T>,

    projection_uniforms: ProjectionUniforms<T>,
    uniform_color: Option<T::UniformLocation>,
    uniform_light_color: Option<T::UniformLocation>,
    uniform_light_direction: Option<T::UniformLocation>,

    pub attrib_position: Option<u32>,
    pub attrib_normal: Option<u32>,
}

impl<T: GL> ShadedProgram<T> {
    pub fn new(
        gl: Rc<T>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<ShadedProgram<T>, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), &vertex_shader, &fragment_shader)?;

        let gl = &gl;
        let projection_uniforms: ProjectionUniforms<T> = ProjectionUniforms::new(&gl, &program);
        unsafe {
            let uniform_color = gl.get_uniform_location(program.program, "color");
            let uniform_light_color = gl.get_uniform_location(program.program, "lightColor");
            let uniform_light_direction =
                gl.get_uniform_location(program.program, "lightDirection");
            let attrib_position = gl.get_attrib_location(program.program, "position");
            let attrib_normal = gl.get_attrib_location(program.program, "normal");

            Ok(ShadedProgram {
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
        projection_params: &ProjectionParams,
        shading_params: &ShadingParams,
        color: &Vector4,
    ) {
        let gl = &self.program.gl;
        self.projection_uniforms.bind(&gl, &projection_params);
        unsafe {
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<T>(&self.uniform_color),
                color.as_ref(),
            );
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<T>(&self.uniform_light_color),
                shading_params.light_color.as_ref(),
            );
            gl.uniform_4_f32_slice(
                borrow_uniform_location::<T>(&self.uniform_light_direction),
                shading_params.light_direction.as_ref(),
            );
        }
    }
}

impl<T: GL> Bindable for ShadedProgram<T> {
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

pub struct EdgeProgram<T: GL> {
    program: Program<T>,

    projection_uniforms: ProjectionUniforms<T>,

    pub attrib_position: Option<u32>,
    pub attrib_colors: Option<u32>,
}

impl<T: GL> EdgeProgram<T> {
    pub fn new(
        gl: Rc<T>,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<EdgeProgram<T>, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), &vertex_shader, &fragment_shader)?;
        let gl = &gl;
        let projection_uniforms: ProjectionUniforms<T> = ProjectionUniforms::new(&gl, &program);
        let attrib_position = unsafe { gl.get_attrib_location(program.program, "position") };
        let attrib_colors = unsafe { gl.get_attrib_location(program.program, "color") };
        Ok(EdgeProgram {
            program,
            projection_uniforms,
            attrib_position,
            attrib_colors,
        })
    }

    pub fn bind_uniforms(&self, projection_params: &ProjectionParams) {
        let gl = &self.program.gl;
        self.projection_uniforms.bind(&gl, &projection_params);
    }
}

impl<T: GL> Bindable for EdgeProgram<T> {
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

pub struct ProgramManager<T: GL> {
    pub solid: ShadedProgram<T>,
    pub solid_flat: ShadedProgram<T>,
    pub edge: EdgeProgram<T>,
}

impl<T: GL> ProgramManager<T> {
    pub fn new(gl: Rc<T>) -> Result<ProgramManager<T>, ShaderError> {
        let solid_fs = str::from_utf8(include_bytes!("../shaders/default.fs")).unwrap();
        let solid_vs = str::from_utf8(include_bytes!("../shaders/default.vs")).unwrap();
        let solid_fs_with_bfc = solid_fs.replace("##IS_BFC_CERTIFIED##", "true");
        let solid_fs_without_bfc = solid_fs.replace("##IS_BFC_CERTIFIED##", "false");
        let solid = ShadedProgram::new(Rc::clone(&gl), &solid_vs, &solid_fs_with_bfc)?;
        let solid_flat = ShadedProgram::new(Rc::clone(&gl), &solid_vs, &solid_fs_without_bfc)?;

        let edge_fs = str::from_utf8(include_bytes!("../shaders/edge.fs")).unwrap();
        let edge_vs = str::from_utf8(include_bytes!("../shaders/edge.vs")).unwrap();
        let edge = EdgeProgram::new(Rc::clone(&gl), &edge_vs, &edge_fs)?;

        Ok(ProgramManager {
            solid,
            solid_flat,
            edge,
        })
    }
}

pub enum ProgramKind<'a, T: GL> {
    Solid(&'a ShadedProgram<T>),
    SolidFlat(&'a ShadedProgram<T>),
    Edge(&'a EdgeProgram<T>),
}

impl<'a, T: GL> ProgramKind<'a, T> {
    pub fn unbind(&self) {
        match self {
            Self::Solid(e) | Self::SolidFlat(e) => e.unbind(),
            Self::Edge(e) => e.unbind(),
        };
    }
}
