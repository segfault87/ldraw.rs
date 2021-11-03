use std::{
    collections::HashMap,
    rc::Rc,
    vec::Vec,
};

use cgmath::{
    Deg,
    PerspectiveFov,
    Point3,
    Rad,
    SquareMatrix,
    prelude::*
};
use glow::HasContext;
use ldraw::{
    color::ColorReference,
    Matrix3, Matrix4, PartAlias, Vector3, Vector4
};

use crate::{
    display_list::{DisplayList, DisplayItem},
    part::Part,
    shader::{DefaultProgramInstancingKind, ProgramManager},
    utils::derive_normal_matrix,
};

pub struct ProjectionData {
    pub projection: Matrix4,
    pub model_matrix: Vec<Matrix4>,
    pub model_view: Matrix4,
    pub normal_matrix: Matrix3,
    pub view_matrix: Matrix4,
    pub orthographic: bool,
}

impl Default for ProjectionData {
    fn default() -> Self {
        ProjectionData {
            projection: Matrix4::identity(),
            model_matrix: vec![Matrix4::identity()],
            model_view: Matrix4::identity(),
            normal_matrix: Matrix3::identity(),
            view_matrix: Matrix4::identity(),
            orthographic: false,
        }
    }
}

impl ProjectionData {
    pub fn derive_normal_matrix(&self) -> Matrix3 {
        derive_normal_matrix(&self.model_view)
    }

    pub fn update_projection_matrix(&mut self, proj: &Matrix4) {
        self.projection = proj.clone();
    }

    fn update_model_view_and_normal_matrix(&mut self) {
        self.model_view = self.view_matrix * self.model_matrix.last().unwrap();
        self.normal_matrix = derive_normal_matrix(&self.model_view);
    }

    pub fn update_view_matrix(&mut self, camera: &Matrix4) {
        self.view_matrix = camera.clone();
        self.update_model_view_and_normal_matrix();
    }

    pub fn push_model_matrix(&mut self, m: &Matrix4) {
        let top = self.model_matrix.last().unwrap().clone();
        let transformed = m * top;
        self.model_matrix.push(transformed);
        self.update_model_view_and_normal_matrix();
    }

    pub fn pop_model_matrix(&mut self) {
        if self.model_matrix.len() > 1 {
            self.model_matrix.pop();
            self.update_model_view_and_normal_matrix();
        }
    }
}

#[derive(Debug)]
pub struct ShadingData {
    pub diffuse: Vector3,
    pub emissive: Vector3,
    pub roughness: f32,
    pub metalness: f32,
    pub opacity: f32,
}

impl Default for ShadingData {
    fn default() -> Self {
        ShadingData {
            diffuse: Vector3::new(1.0, 1.0, 1.0),
            emissive: Vector3::zero(),
            roughness: 0.3,
            metalness: 0.0,
            opacity: 1.0,
        }
    }
}

pub struct Camera {
    pub position: Point3<f32>,
    pub look_at: Point3<f32>,
    pub up: Vector3,
    pub fov: Deg<f32>,
}

impl Camera {
    pub fn new(
        position: Point3<f32>, look_at: Point3<f32>,
        fov: Deg<f32>
    ) -> Self {
        Camera {
            position,
            look_at,
            up: Vector3::new(0.0, -1.0, 0.0),
            fov
        }
    }

    pub fn derive_projection_matrix(&self, aspect_ratio: f32) -> Matrix4 {
        Matrix4::from(PerspectiveFov {
            fovy: Rad::from(self.fov),
            aspect: aspect_ratio,
            near: 0.1,
            far: 100000.0
        })
    }

    pub fn derive_view_matrix(&self) -> Matrix4 {
        Matrix4::look_at_rh(self.position, self.look_at, self.up)
    }
}

pub struct RenderingContext<GL: HasContext> {
    gl: Rc<GL>,

    pub program_manager: ProgramManager<GL>,
    width: u32,
    height: u32,
    
    pub camera: Camera,
    pub projection_data: ProjectionData,
    pub shading_data: ShadingData,

    envmap: Option<GL::Texture>,
}

impl<GL: HasContext> RenderingContext<GL> {
    pub fn new(gl: Rc<GL>, program_manager: ProgramManager<GL>) -> Self {
        let envmap = unsafe {
            let envmap = match gl.create_texture() {
                Ok(e) => Some(e),
                Err(msg) => {
                    println!("Failed creating envmap texture: {}", msg);
                    None
                }
            };
            gl.bind_texture(glow::TEXTURE_2D, envmap);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA as i32, 768, 768, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(include_bytes!("../assets/cubemap.bin")));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);

            envmap
        };

        RenderingContext {
            gl: Rc::clone(&gl),
            camera: Camera::new(
                Point3::new(0.0, -200.0, -1200.0),
                Point3::new(0.0, 0.0, 0.0),
                Deg(45.0),
            ),
            program_manager,
            width: 1,
            height: 1,
            projection_data: ProjectionData::default(),
            shading_data: ShadingData::default(),
            envmap,
        }
    }

    pub fn update_camera(&mut self) {
        self.projection_data.update_projection_matrix(
            &self.camera.derive_projection_matrix(self.width as f32 / self.height as f32)
        );
        self.projection_data.update_view_matrix(
            &self.camera.derive_view_matrix()
        );
    }

    pub fn upload_shading_data(&self) {
        self.program_manager.bind_envmap(&self.envmap);
    }

    pub fn set_initial_state(&self) {
        let gl = &self.gl;
        unsafe {
            gl.clear_color(1.0, 1.0, 1.0, 1.0);
            gl.line_width(2.0);
            gl.cull_face(glow::BACK);
            gl.enable(glow::CULL_FACE);
            gl.enable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.depth_func(glow::LEQUAL);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.update_camera();
        unsafe {
            self.gl.viewport(0, 0, width as i32, height as i32);
        }
    }

    pub fn render_instance(
        &mut self, part: &Part<GL>, display_item: &mut DisplayItem<GL>,
        semitransparent: bool
    ) {
        let gl = &self.gl;
        let part_buffer = &part.part;

        let mut instance_buffer = if semitransparent {
            &mut display_item.semitransparent
        } else {
            &mut display_item.opaque
        };

        if instance_buffer.count == 0 {
            return;
        }

        if let Some(uncolored_index) = &part_buffer.uncolored_index {
            let program = self.program_manager.get_default_program(
                DefaultProgramInstancingKind::InstancedWithColors, true
            );

            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&mut instance_buffer);
            bind.bind_instanced_color_data(&mut instance_buffer);

            unsafe {
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    uncolored_index.start as i32,
                    uncolored_index.span as i32,
                    instance_buffer.count as i32
                );
            }
        }
        if let Some(uncolored_without_bfc_index) = &part_buffer.uncolored_without_bfc_index {
            let program = self.program_manager.get_default_program(
                DefaultProgramInstancingKind::InstancedWithColors, false
            );

            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&mut instance_buffer);
            bind.bind_instanced_color_data(&mut instance_buffer);

            unsafe {
                gl.disable(glow::CULL_FACE);
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    uncolored_without_bfc_index.start as i32,
                    uncolored_without_bfc_index.span as i32,
                    instance_buffer.count as i32
                );
                gl.enable(glow::CULL_FACE);
            }
        }
        let subparts = if semitransparent {
            &part_buffer.semitransparent_indices
        } else {
            &part_buffer.opaque_indices
        };
        for (group, indices) in subparts.iter() {
            let program = self.program_manager.get_default_program(
                DefaultProgramInstancingKind::Instanced, group.bfc
            );
            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&mut instance_buffer);
            let color = match &group.color_ref {
                ColorReference::Material(m) => m.color.into(),
                _ => continue,
            };
            bind.bind_non_instanced_color_data(&color);
            
            unsafe {
                if !group.bfc {
                    gl.disable(glow::CULL_FACE);
                }
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    indices.start as i32,
                    indices.span as i32,
                    instance_buffer.count as i32
                );
                if !group.bfc {
                    gl.enable(glow::CULL_FACE);
                }
            }
        }

        if let Some(edges) = &part_buffer.edges {
            let program = self.program_manager.get_edge_program(true);

            let bind = program.bind(&self.projection_data);
            bind.bind_attribs(&edges);
            bind.bind_instanced_attribs(&mut instance_buffer);

            unsafe {
                gl.draw_arrays_instanced(
                    glow::LINES,
                    0,
                    edges.length as i32,
                    instance_buffer.count as i32
                );
            }
        }

        if let Some(optional_edges) = &part_buffer.optional_edges {
            let program = self.program_manager.get_optional_edge_program(true);

            let bind = program.bind(&self.projection_data);
            bind.bind_attribs(&optional_edges);
            bind.bind_instanced_attribs(&mut instance_buffer);

            unsafe {
                gl.draw_arrays_instanced(
                    glow::LINES,
                    0,
                    optional_edges.length as i32,
                    instance_buffer.count as i32
                );
            }
        }
    }

    pub fn render_single_part(
        &mut self,
        part: &Part<GL>, color: &ColorReference, semitransparent: bool
    ) {
        let gl = &self.gl;
        let part_buffer = &part.part;

        let material = match color {
            ColorReference::Material(m) => m,
            _ => return,
        };
        let default_color: Vector4 = material.color.into();
        let edge_color: Vector4 = material.edge.into();

        if material.is_semi_transparent() == semitransparent {
            if let Some(uncolored_index) = &part_buffer.uncolored_index {
                let program = self.program_manager.get_default_program(
                    DefaultProgramInstancingKind::NonInstanced, true
                );

                let bind = program.bind(&self.projection_data, &self.shading_data);
                bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
                bind.bind_non_instanced_color_data(&default_color);

                unsafe {
                    gl.draw_arrays(
                        glow::TRIANGLES,
                        uncolored_index.start as i32,
                        uncolored_index.span as i32
                    );
                }
            }
            if let Some(uncolored_without_bfc_index) = &part_buffer.uncolored_without_bfc_index {
                let program = self.program_manager.get_default_program(
                    DefaultProgramInstancingKind::NonInstanced, false
                );

                let bind = program.bind(&self.projection_data, &self.shading_data);
                bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
                bind.bind_non_instanced_color_data(&default_color);

                unsafe {
                    gl.disable(glow::CULL_FACE);
                    gl.draw_arrays(
                        glow::TRIANGLES,
                        uncolored_without_bfc_index.start as i32,
                        uncolored_without_bfc_index.span as i32
                    );
                    gl.enable(glow::CULL_FACE);
                }
            }
        }

        let subparts = if semitransparent {
            &part_buffer.semitransparent_indices
        } else {
            &part_buffer.opaque_indices
        };
        for (group, indices) in subparts.iter() {
            let color = match &group.color_ref {
                ColorReference::Material(m) => m.color.into(),
                _ => continue,
            };

            let program = self.program_manager.get_default_program(
                DefaultProgramInstancingKind::NonInstanced, group.bfc
            );

            let bind = program.bind(&self.projection_data, &self.shading_data);
            bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
            bind.bind_non_instanced_color_data(&color);
            
            unsafe {
                if !group.bfc {
                    gl.disable(glow::CULL_FACE);
                }
                gl.draw_arrays(
                    glow::TRIANGLES,
                    indices.start as i32,
                    indices.span as i32
                );
                if !group.bfc {
                    gl.enable(glow::CULL_FACE);
                }
            }
        }

        if !semitransparent {
            if let Some(edges) = &part_buffer.edges {
                let program = self.program_manager.get_edge_program(false);

                let bind = program.bind(&self.projection_data);
                bind.bind_attribs(&edges);
                bind.bind_non_instanced_properties(&default_color, &edge_color);

                unsafe {
                    gl.draw_arrays(
                        glow::LINES,
                        0,
                        edges.length as i32
                    );
                }
            }

            if let Some(optional_edges) = &part_buffer.optional_edges {
                let program = self.program_manager.get_optional_edge_program(false);

                let bind = program.bind(&self.projection_data);
                bind.bind_attribs(&optional_edges);
                bind.bind_non_instanced_properties(&default_color, &edge_color);

                unsafe {
                    gl.draw_arrays(
                        glow::LINES,
                        0,
                        optional_edges.length as i32
                    );
                }
            }
        }
    }

    pub fn render_display_list(
        &mut self, parts: &HashMap<PartAlias, Part<GL>>, display_list: &mut DisplayList<GL>,
        semitransparent: bool
    ) {
        for (alias, mut object) in display_list.map.iter_mut() {
            let part = match parts.get(&alias) {
                Some(e) => e,
                None => continue,
            };

            self.render_instance(&part, &mut object, semitransparent);
        }
    }

}

impl<GL: HasContext> Drop for RenderingContext<GL> {
    fn drop(&mut self) {
        unsafe {
            if let Some(e) = self.envmap {
                self.gl.delete_texture(e);
            }
        }
    }
}
