use std::{
    fmt::{Write as FmtWrite},
    io::{BufWriter, Write as IoWrite},
    rc::Rc,
    str,
};

use glow::HasContext;
use ldraw::{Matrix3, Vector4};

use crate::{
    display_list::InstanceBuffer,
    error::ShaderError,
    state::{ProjectionData, ShadingData},
    part::MeshBuffer,
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
            writeln!(buf, "#version 300 es");
        } else {
            writeln!(buf, "#version 330");
        }

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

        write!(buf, "{}", self.source);

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

    // Geometry
    position: Option<u32>,
    normal: Option<u32>,

    // Projection for shading
    view_matrix: Option<GL::UniformLocation>,
    is_orthographic: Option<GL::UniformLocation>,

    // Instancing
    instanced_model_view: Option<u32>,
    instanced_normal_matrix: Option<u32>,
    
    // Instanced colors
    instanced_color: Option<u32>,

    // Non-instancing
    normal_matrix: Option<GL::UniformLocation>,
    color: Option<GL::UniformLocation>,

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

pub struct DefaultProgramBinder<'a, GL: HasContext> {
    gl: Rc<GL>,
    program: &'a DefaultProgram<GL>,
}

impl<'a, GL: HasContext> DefaultProgramBinder<'a, GL> {
    fn new(program: &'a DefaultProgram<GL>) -> Self {
        program.program.use_program();

        DefaultProgramBinder {
            gl: Rc::clone(&program.gl),
            program: &program
        }
    }

    pub fn bind_geometry_data(&self, mesh: &MeshBuffer<GL>) -> bool {
        let gl = &self.gl;
        if mesh.buffer_vertices.is_some() && mesh.buffer_normals.is_some() {
            unsafe {
                gl.bind_vertex_array(mesh.array);

                gl.bind_buffer(glow::ARRAY_BUFFER, mesh.buffer_vertices);
                gl.vertex_attrib_pointer_f32(self.program.position.unwrap(), 3, glow::FLOAT, false, 0, 0);
                gl.enable_vertex_attrib_array(self.program.position.unwrap());

                gl.bind_buffer(glow::ARRAY_BUFFER, mesh.buffer_normals);
                gl.vertex_attrib_pointer_f32(self.program.normal.unwrap(), 3, glow::FLOAT, false, 0, 0);
                gl.enable_vertex_attrib_array(self.program.normal.unwrap());
            }
            true
        } else {
            false
        }
    }

    pub fn bind_instanced_geometry_data(&self, instance_buffer: &mut InstanceBuffer<GL>) {
        let gl = &self.gl;

        instance_buffer.update_buffer(&gl);
        if self.program.instanced_model_view.is_some() && self.program.instanced_normal_matrix.is_some() {
            let instanced_model_view = self.program.instanced_model_view.unwrap();
            let instanced_normal_matrix = self.program.instanced_normal_matrix.unwrap();
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, instance_buffer.model_view_matrices_buffer);
                for i in 0..4 {
                    gl.vertex_attrib_pointer_f32(instanced_model_view + i, 4, glow::FLOAT, false, 4 * 16, (16 * i) as i32);
                    gl.enable_vertex_attrib_array(instanced_model_view + i);
                    gl.vertex_attrib_divisor(instanced_model_view + i, 1);
                }
                gl.bind_buffer(glow::ARRAY_BUFFER, instance_buffer.normal_matrices_buffer);
                for i in 0..3 {
                    gl.vertex_attrib_pointer_f32(instanced_normal_matrix + i, 4, glow::FLOAT, false, 3 * 16, (16 * i) as i32);
                    gl.enable_vertex_attrib_array(instanced_normal_matrix + i);
                    gl.vertex_attrib_divisor(instanced_normal_matrix + i, 1);
                }
            }

        }
    }

    pub fn bind_non_instanced_data(&self, normal_matrix: &Matrix3, color: &Vector4) {
        let gl = &self.gl;

        unsafe {
            gl.uniform_matrix_4_f32_slice(
                self.program.normal_matrix.as_ref(),
                false,
                AsRef::<[f32; 9]>::as_ref(&normal_matrix)
            );
        }

        self.bind_non_instanced_color_data(&color);
    }

    pub fn bind_non_instanced_color_data(&self, color: &Vector4) {
        let gl = &self.gl;

        unsafe {
            gl.uniform_4_f32_slice(
                self.program.color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&color)
            )
        }
    }

    pub fn bind_instanced_color_data(&self, instance_buffer: &mut InstanceBuffer<GL>) {
        let gl = &self.gl;

        instance_buffer.update_buffer(&gl);
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

        unsafe {
            if let Some(a) = self.program.position {
                gl.disable_vertex_attrib_array(a);
            }
            if let Some(a) = self.program.normal {
                gl.disable_vertex_attrib_array(a);
            }
            if let Some(a) = self.program.instanced_model_view {
                gl.disable_vertex_attrib_array(a);
                gl.disable_vertex_attrib_array(a + 1);
                gl.disable_vertex_attrib_array(a + 2);
                gl.disable_vertex_attrib_array(a + 3);
            }
            if let Some(a) = self.program.instanced_normal_matrix {
                gl.disable_vertex_attrib_array(a);
                gl.disable_vertex_attrib_array(a + 1);
                gl.disable_vertex_attrib_array(a + 2);
            }
            if let Some(a) = self.program.instanced_color {
                gl.disable_vertex_attrib_array(a);
            }
        }
    }
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

                position: gl.get_attrib_location(program.program, "position"),
                normal: gl.get_attrib_location(program.program, "normal"),

                view_matrix: gl.get_uniform_location(program.program, "viewMatrix"),
                is_orthographic: gl.get_uniform_location(program.program, "isOrthographic"),

                instanced_model_view: gl.get_attrib_location(program.program, "instancedModelView"),
                instanced_normal_matrix: gl.get_attrib_location(program.program, "instancedNormalMatrix"),

                instanced_color: gl.get_attrib_location(program.program, "instancedColor"),

                normal_matrix: gl.get_uniform_location(program.program, "normalMatrix"),
                color: gl.get_uniform_location(program.program, "color"),

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

    pub fn bind_projection_data(&self, projection_data: &ProjectionData) {
        let gl = &self.gl;
        unsafe {
            gl.uniform_matrix_4_f32_slice(
                self.projection.as_ref(),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_data.projection)
            );
            gl.uniform_matrix_4_f32_slice(
                self.model_view.as_ref(),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_data.model_view.last().unwrap())
            );
            gl.uniform_matrix_4_f32_slice(
                self.view_matrix.as_ref(),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_data.view_matrix)
            );
            gl.uniform_1_i32(
                self.is_orthographic.as_ref(),
                if projection_data.orthographic { 1 } else { 0 }
            );
        }
    }

    pub fn bind_shading_data(&self, shading_data: &ShadingData) {
        let gl = &self.gl;
        unsafe {
            // Shading
            gl.uniform_3_f32_slice(
                self.diffuse.as_ref(),
                AsRef::<[f32; 3]>::as_ref(&shading_data.diffuse)
            );
            gl.uniform_3_f32_slice(
                self.emissive.as_ref(),
                AsRef::<[f32; 3]>::as_ref(&shading_data.emissive)
            );
            gl.uniform_3_f32_slice(
                self.specular.as_ref(),
                AsRef::<[f32; 3]>::as_ref(&shading_data.specular)
            );
            gl.uniform_1_f32(
                self.shininess.as_ref(),
                shading_data.shininess
            );
            gl.uniform_1_f32(
                self.opacity.as_ref(),
                shading_data.opacity
            );
            gl.uniform_3_f32_slice(
                self.ambient_light_color.as_ref(),
                AsRef::<[f32; 3]>::as_ref(&shading_data.ambient_light_color)
            );
            for i in 0..9 {
                gl.uniform_3_f32_slice(
                    self.light_probe[i].as_ref(),
                    AsRef::<[f32; 3]>::as_ref(&shading_data.light_probe[i])
                );
            }

            // Lighting
            for (shader, data) in self.directional_lights.iter().zip(&shading_data.directional_lights) {
                gl.uniform_3_f32_slice(
                    shader.direction.as_ref(),
                    AsRef::<[f32; 3]>::as_ref(&data.direction)
                );
                gl.uniform_3_f32_slice(
                    shader.color.as_ref(),
                    AsRef::<[f32; 3]>::as_ref(&data.color)
                );
            }
            for (shader, data) in self.point_lights.iter().zip(&shading_data.point_lights) {
                gl.uniform_3_f32_slice(
                    shader.position.as_ref(),
                    AsRef::<[f32; 3]>::as_ref(&data.position)
                );
                gl.uniform_3_f32_slice(
                    shader.color.as_ref(),
                    AsRef::<[f32; 3]>::as_ref(&data.color)
                );
                gl.uniform_1_f32(
                    shader.distance.as_ref(),
                    data.distance
                );
                gl.uniform_1_f32(
                    shader.decay.as_ref(),
                    data.decay
                );
            }
        }
    }

    pub fn bind<'a>(&'a self) -> DefaultProgramBinder<'a, GL> {
        DefaultProgramBinder::new(&self)
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

    pub fn use_program(&self) {
        unsafe {
            self.gl.use_program(Some(self.program.program));
        }
    }

    pub fn bind_projection_data(&self, projection_data: &ProjectionData) {
        let gl = &self.gl;
        unsafe {
            gl.uniform_matrix_4_f32_slice(
                self.projection.as_ref(),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_data.projection)
            );
            gl.uniform_matrix_4_f32_slice(
                self.model_view.as_ref(),
                false,
                AsRef::<[f32; 16]>::as_ref(&projection_data.model_view.last().unwrap())
            );
        }
    }

    pub fn bind_non_instanced_properties(&self, color: &Vector4, edge_color: &Vector4) {
        let gl = &self.gl;
        unsafe {
            gl.uniform_4_f32_slice(
                self.default_color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&color)
            );
            gl.uniform_4_f32_slice(
                self.edge_color.as_ref(),
                AsRef::<[f32; 4]>::as_ref(&edge_color)
            );
        }
    }
}

pub struct ProgramManager<GL: HasContext> {
    pub num_directional_lights: usize,
    pub num_point_lights: usize,

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
            (DefaultProgramInstancingKind::NonInstanced, true) => &self.default,
            (DefaultProgramInstancingKind::Instanced, true) => &self.default_instanced,
            (DefaultProgramInstancingKind::InstancedWithColors, true) => &self.default_instanced_with_colors,
            (DefaultProgramInstancingKind::NonInstanced, false) => &self.default_without_bfc,
            (DefaultProgramInstancingKind::Instanced, false) => &self.default_without_bfc_instanced,
            (DefaultProgramInstancingKind::InstancedWithColors, false) => &self.default_without_bfc_instanced_with_colors,
        }
    }

    pub fn bind_projection_data(&self, projection_data: &ProjectionData) {
        self.default.bind_projection_data(&projection_data);
        self.default_instanced.bind_projection_data(&projection_data);
        self.default_instanced_with_colors.bind_projection_data(&projection_data);
        self.default_without_bfc.bind_projection_data(&projection_data);
        self.default_without_bfc_instanced.bind_projection_data(&projection_data);
        self.default_without_bfc_instanced_with_colors.bind_projection_data(&projection_data);
        self.edge.bind_projection_data(&projection_data);
        self.edge_instanced.bind_projection_data(&projection_data);
    }

    pub fn bind_shading_data(&self, shading_data: &ShadingData) {
        self.default.bind_shading_data(&shading_data);
        self.default_instanced.bind_shading_data(&shading_data);
        self.default_instanced_with_colors.bind_shading_data(&shading_data);
    }


}
