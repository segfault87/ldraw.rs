use std::{
    convert::TryInto,
    fmt::{Write as FmtWrite},
    io::{BufWriter, Write as IoWrite},
    rc::Rc,
    str,
};

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

        for line in self.source.lines() {
            writeln!(buf, "{}", line);
            if line.starts_with("#version ") {
                for (flag, value) in &self.flags {
                    match value {
                        Some(v) => {
                            writeln!(buf, "#define {} {}", flag, v);
                        }
                        None => {
                            writeln!(buf, "#define {}", flag);
                        }
                    };
                }
            }
        }

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
        let vs = Self::compile_shader(&gl, &vertex_shader, glow::VERTEX_SHADER)?;
        let fs = Self::compile_shader(&gl, &fragment_shader, glow::FRAGMENT_SHADER)?;

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

struct DirectionalLightUniforms<GL: HasContext> {
    direction: Option<GL::UniformLocation>,
    color: Option<GL::UniformLocation>,
}

impl<GL: HasContext> DirectionalLightUniforms<GL> {

    pub fn new(gl: &GL, program: &Program<GL>, index: usize) -> Self {
        let mut direction_key = String::new();
        write!(direction_key, "directionalLights[{}].direction", index);
        let mut color_key = String::new();
        write!(color_key, "directionalLights[{}].color", index);

        unsafe {
            DirectionalLightUniforms {
                direction: gl.get_uniform_location(program.program, &direction_key),
                color: gl.get_uniform_location(program.program, &color_key),
            }
        }
    }

}

struct PointLightUniforms<GL: HasContext> {
    position: Option<GL::UniformLocation>,
    color: Option<GL::UniformLocation>,
    distance: Option<GL::UniformLocation>,
    decay: Option<GL::UniformLocation>,
}

impl<GL: HasContext> PointLightUniforms<GL> {

    pub fn new(gl: &GL, program: &Program<GL>, index: usize) -> Self {
        let mut position_key = String::new();
        write!(&mut position_key, "pointLights[{}].position", index);
        let mut color_key = String::new();
        write!(&mut color_key, "pointLights[{}].color", index);
        let mut distance_key = String::new();
        write!(&mut distance_key, "pointLights[{}].distance", index);
        let mut decay_key = String::new();
        write!(&mut decay_key, "pointLights[{}].decay", index);


        unsafe {
            PointLightUniforms {
                position: gl.get_uniform_location(program.program, &position_key),
                color: gl.get_uniform_location(program.program, &color_key),
                distance: gl.get_uniform_location(program.program, &distance_key),
                decay: gl.get_uniform_location(program.program, &decay_key),
            }
        }
    }

}

pub struct DefaultProgram<GL: HasContext> {
    gl: Rc<GL>,
    program: Program<GL>,

    // Basic projection
    projection: Option<GL::UniformLocation>,
    model_view: Option<GL::UniformLocation>,

    // Projection for shading
    view_matrix: Option<GL::UniformLocation>,
    camera_position: Option<GL::UniformLocation>,
    is_orthographic: Option<GL::UniformLocation>,

    // Instancing
    instanced_model_view: Option<u32>,
    instanced_normal_matrix: Option<u32>,
    
    // Instanced colors
    instanced_color: Option<u32>,

    // Non-instancing
    normal_matrix: Option<GL::UniformLocation>,

    // Materials
    diffuse: Option<GL::UniformLocation>,
    emissive: Option<GL::UniformLocation>,
    specular: Option<GL::UniformLocation>,
    shininess: Option<GL::UniformLocation>,
    opacity: Option<GL::UniformLocation>,

    // Lighting
    directional_lights: Vec<DirectionalLightUniforms<GL>>,
    point_lights: Vec<PointLightUniforms<GL>>,
    ambient_light_color: Option<GL::UniformLocation>,
    light_probe: [Option<GL::UniformLocation>; 9],
}

impl<GL: HasContext> DefaultProgram<GL> {
    fn new(
        gl: Rc<GL>, vertex_shader: &ShaderSource, fragment_shader: &ShaderSource,
        num_directional_lights: usize, num_point_lights: usize
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), vertex_shader, fragment_shader)?;

        let cloned_gl = Rc::clone(&gl);
        let gl: &GL = &gl;

        unsafe {
            Ok(DefaultProgram {
                gl: cloned_gl,

                projection: gl.get_uniform_location(program.program, "projection"),
                model_view: gl.get_uniform_location(program.program, "modelView"),

                view_matrix: gl.get_uniform_location(program.program, "viewMatrix"),
                camera_position: gl.get_uniform_location(program.program, "cameraPosition"),
                is_orthographic: gl.get_uniform_location(program.program, "isOrthographic"),

                instanced_model_view: gl.get_attrib_location(program.program, "instancedModelView"),
                instanced_normal_matrix: gl.get_attrib_location(program.program, "instancedNormalMatrix"),

                instanced_color: gl.get_attrib_location(program.program, "instancedColor"),

                normal_matrix: gl.get_uniform_location(program.program, "normalMatrix"),

                diffuse: gl.get_uniform_location(program.program, "diffuse"),
                emissive: gl.get_uniform_location(program.program, "emissive"),
                specular: gl.get_uniform_location(program.program, "specular"),
                shininess: gl.get_uniform_location(program.program, "shininess"),
                opacity: gl.get_uniform_location(program.program, "opacity"),

                directional_lights: (0..num_directional_lights).map(|i| DirectionalLightUniforms::new(gl, &program, i)).collect(),
                point_lights: (0..num_point_lights).map(|i| PointLightUniforms::new(gl, &program, i)).collect(),
                ambient_light_color: gl.get_uniform_location(program.program, "ambientLightColor"),
                light_probe: [0, 1, 2, 3, 4, 5, 6, 7, 8].map(|i| {
                    let mut key = String::new();
                    write!(&mut key, "lightProbe[{}]", i);
                    gl.get_uniform_location(program.program, &key)
                }),

                program,
            })
        }
    }

    pub fn use_program(&self) {
        unsafe {
            self.gl.use_program(Some(self.program.program));
        }
    }

    pub fn bind_projection_data(&self, projection_data: &ProjectionData) {
        unsafe {
            
        }
    }
}

#[derive(Copy, Clone)]
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
    instanced_model_view: Option<u32>,

    // Non-instancing
    default_color: Option<GL::UniformLocation>,
    edge_color: Option<GL::UniformLocation>,
}

impl<GL: HasContext> EdgeProgram<GL> {
    fn new(
        gl: Rc<GL>, vertex_shader: &ShaderSource, fragment_shader: &ShaderSource
    ) -> Result<Self, ShaderError> {
        let program = Program::compile(Rc::clone(&gl), vertex_shader, fragment_shader)?;

        let cloned_gl = Rc::clone(&gl);
        let gl: &GL = &gl;

        unsafe {
            Ok(EdgeProgram {
                gl: cloned_gl,

                projection: gl.get_uniform_location(program.program, "projection"),
                model_view: gl.get_uniform_location(program.program, "modelView"),
        
                position: gl.get_attrib_location(program.program, "position"),
                color: gl.get_attrib_location(program.program, "color"),
        
                instanced_color: gl.get_attrib_location(program.program, "instancedColor"),
                instanced_edge_color: gl.get_attrib_location(program.program, "instancedEdgeColor"),
                instanced_model_view: gl.get_attrib_location(program.program, "instancedModelView"),

                default_color: gl.get_uniform_location(program.program, "defaultColor"),
                edge_color: gl.get_uniform_location(program.program, "edgeColor"),

                program
            })
        }
    }
}

pub struct ProgramManager<GL: HasContext> {
    num_directional_lights: usize,
    num_point_lights: usize,

    pub default: DefaultProgram<GL>,
    pub default_instanced: DefaultProgram<GL>,
    pub default_instanced_with_colors: DefaultProgram<GL>,

    pub default_without_bfc: DefaultProgram<GL>,
    pub default_without_bfc_instanced: DefaultProgram<GL>,
    pub default_without_bfc_instanced_with_colors: DefaultProgram<GL>,

    pub edge: EdgeProgram<GL>,
    pub edge_instanced: EdgeProgram<GL>,
}

impl<GL: HasContext> ProgramManager<GL> {
    pub fn new(gl: Rc<GL>, num_directional_lights: usize, num_point_lights: usize) -> Result<ProgramManager<GL>, ShaderError> {
        let default_fs = ShaderSource::new(String::from_utf8(include_bytes!("../shaders/default.fs").to_vec()).unwrap())
            .with_value("NUM_POINT_LIGHTS", num_point_lights.to_string())
            .with_value("NUM_DIRECTIONAL_LIGHTS", num_directional_lights.to_string());
        let default_vs = ShaderSource::new(String::from_utf8(include_bytes!("../shaders/default.vs").to_vec()).unwrap());

        let default = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs,
            &default_fs,
            num_directional_lights, num_point_lights
        )?;
        let default_instanced = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs.clone().with_flag("USE_INSTANCING"),
            &default_fs.clone().with_flag("USE_INSTANCING"),
            num_directional_lights, num_point_lights
        )?;
        let default_instanced_with_colors = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs.clone().with_flag("USE_INSTANCING").with_flag("USE_INSTANCED_COLORS"),
            &default_fs.clone().with_flag("USE_INSTANCING").with_flag("USE_INSTANCED_COLORS"),
            num_directional_lights, num_point_lights
        )?;
        let default_without_bfc = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs.clone().with_flag("WITHOUT_BFC"),
            &default_fs.clone().with_flag("WITHOUT_BFC"),
            num_directional_lights, num_point_lights
        )?;
        let default_without_bfc_instanced = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs.clone().with_flag("WITHOUT_BFC").with_flag("USE_INSTANCING"),
            &default_fs.clone().with_flag("WITHOUT_BFC").with_flag("USE_INSTANCING"),
            num_directional_lights, num_point_lights
        )?;
        let default_without_bfc_instanced_with_colors = DefaultProgram::new(
            Rc::clone(&gl),
            &default_vs.clone().with_flag("WITHOUT_BFC").with_flag("USE_INSTANCING").with_flag("USE_INSTANCED_COLORS"),
            &default_fs.clone().with_flag("WITHOUT_BFC").with_flag("USE_INSTANCING").with_flag("USE_INSTANCED_COLORS"),
            num_directional_lights, num_point_lights
        )?;

        let edge_fs = ShaderSource::new(String::from_utf8(include_bytes!("../shaders/edge.fs").to_vec()).unwrap());
        let edge_vs = ShaderSource::new(String::from_utf8(include_bytes!("../shaders/edge.vs").to_vec()).unwrap());

        let edge = EdgeProgram::new(
            Rc::clone(&gl),
            &edge_vs,
            &edge_fs
        )?;
        let edge_instanced = EdgeProgram::new(
            Rc::clone(&gl),
            &edge_vs.clone().with_flag("USE_INSTANCING"),
            &edge_fs.clone().with_flag("USE_INSTANCING")
        )?;

        Ok(ProgramManager {
            num_directional_lights,
            num_point_lights,

            default,
            default_instanced,
            default_instanced_with_colors,
            default_without_bfc,
            default_without_bfc_instanced,
            default_without_bfc_instanced_with_colors,

            edge,
            edge_instanced
        })
    }

    pub fn get_default_program<'a>(&'a self, instancing_kind: DefaultProgramInstancingKind, bfc: bool) -> &'a DefaultProgram<GL> {
        match (instancing_kind, bfc) {
            (DefaultProgramInstancingKind::NonInstanced, false) => &self.default,
            (DefaultProgramInstancingKind::Instanced, false) => &self.default_instanced,
            (DefaultProgramInstancingKind::InstancedWithColors, false) => &self.default_instanced_with_colors,
            (DefaultProgramInstancingKind::NonInstanced, true) => &self.default_without_bfc,
            (DefaultProgramInstancingKind::Instanced, true) => &self.default_without_bfc_instanced,
            (DefaultProgramInstancingKind::InstancedWithColors, true) => &self.default_without_bfc_instanced_with_colors,
        }
    }
}

