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
    pub model_view: Vec<Matrix4>,
    pub view_matrix: Matrix4,
    pub orthographic: bool,
}

impl Default for ProjectionData {
    fn default() -> Self {
        ProjectionData {
            projection: Matrix4::identity(),
            model_view: vec![Matrix4::identity()],
            view_matrix: Matrix4::identity(),
            orthographic: false,
        }
    }
}

impl ProjectionData {
    pub fn derive_normal_matrix(&self) -> Matrix3 {
        derive_normal_matrix(self.model_view.last().unwrap())
    }

    pub fn update_projection_matrix(&mut self, proj: &Matrix4) {
        self.projection = proj.clone();
        self.view_matrix = proj.invert().unwrap_or(Matrix4::identity());
    }

    pub fn push_model_view_matrix(&mut self, m: &Matrix4) {
        let top = self.model_view.last().unwrap().clone();
        self.model_view.push(top * m);
    }

    pub fn pop_model_view_matrix(&mut self) {
        if self.model_view.len() > 1 {
            self.model_view.pop();
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirectionalLight {
    pub direction: Vector3,
    pub color: Vector3,
}

impl DirectionalLight {
    pub fn new(direction: &Vector3, color: &Vector3) -> Self {
        DirectionalLight {
            direction: direction.clone(),
            color: color.clone(),
        }
    }
}

impl Default for DirectionalLight {
    fn default() -> Self {
        DirectionalLight {
            direction: Vector3::new(0.0, -1.0, 0.0),
            color: Vector3::new(0.75, 0.75, 0.75),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PointLight {
    pub position: Vector3,
    pub color: Vector3,
    pub distance: f32,
    pub decay: f32,
}

impl PointLight {
    pub fn new(position: &Vector3, color: &Vector3, distance: f32, decay: f32) -> Self {
        PointLight {
            position: position.clone(),
            color: color.clone(),
            distance,
            decay,
        }
    }
}

impl Default for PointLight {
    fn default() -> Self {
        PointLight {
            position: Vector3::zero(),
            color: Vector3::new(0.75, 0.75, 0.75),
            distance: 100.0,
            decay: 25.0,
        }
    }
}

#[derive(Debug)]
pub struct ShadingData {
    pub directional_lights: Vec<DirectionalLight>,
    pub point_lights: Vec<PointLight>,
    pub ambient_light_color: Vector3,
    pub light_probe: [Vector3; 9],
    pub diffuse: Vector3,
    pub emissive: Vector3,
    pub specular: Vector3,
    pub shininess: f32,
    pub opacity: f32,
}

impl ShadingData {
    pub fn new(num_directional_lights: usize, num_point_lights: usize) -> Self {
        ShadingData {
            directional_lights: vec![DirectionalLight::default(); num_directional_lights],
            point_lights: vec![PointLight::default(); num_point_lights],
            ambient_light_color: Vector3::new(0.25, 0.25, 0.25),
            light_probe: [Vector3::zero(); 9],
            diffuse: Vector3::new(0.5, 0.5, 0.5),
            emissive: Vector3::zero(),
            specular: Vector3::new(1.0, 1.0, 1.0),
            shininess: 0.2,
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
        }) * Matrix4::look_at_rh(self.position, self.look_at, self.up)
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
}

impl<GL: HasContext> RenderingContext<GL> {
    pub fn new(gl: Rc<GL>, program_manager: ProgramManager<GL>) -> Self {
        let num_directional_lights = program_manager.num_directional_lights;
        let num_point_lights = program_manager.num_point_lights;
        RenderingContext {
            gl: Rc::clone(&gl),
            camera: Camera::new(
                Point3::new(0.0, -100.0, -300.0),
                Point3::new(0.0, 0.0, 0.0),
                Deg(45.0),
            ),
            program_manager,
            width: 1,
            height: 1,
            projection_data: ProjectionData::default(),
            shading_data: ShadingData::new(
                num_directional_lights, num_point_lights
            ),
        }
    }

    pub fn update_camera(&mut self) {
        self.projection_data.update_projection_matrix(
            &self.camera.derive_projection_matrix(self.width as f32 / self.height as f32)
        );
        self.upload_projection_data();
    }

    fn upload_projection_data(&self) {
        self.program_manager.bind_projection_data(&self.projection_data);
    }

    pub fn upload_shading_data(&self) {
        self.program_manager.bind_shading_data(&self.shading_data);
    }

    pub fn set_initial_state(&self) {
        let gl = &self.gl;
        unsafe {
            gl.clear_color(1.0, 1.0, 1.0, 1.0);
            gl.line_width(1.0);
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

    pub fn render_instance_mesh(
        &self, part: &Part<GL>, display_item: &mut DisplayItem<GL>,
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

            let bind = program.bind();
            bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&mut instance_buffer);
            bind.bind_instanced_color_data(&mut instance_buffer);

            unsafe {
                println!("uncolored {} {} {}", uncolored_index.start, uncolored_index.span, instance_buffer.count);
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

            let bind = program.bind();
            bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&mut instance_buffer);
            bind.bind_instanced_color_data(&mut instance_buffer);

            unsafe {
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    uncolored_without_bfc_index.start as i32,
                    uncolored_without_bfc_index.span as i32,
                    instance_buffer.count as i32
                );
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

            let bind = program.bind();
            bind.bind_geometry_data(&part_buffer.mesh.as_ref().unwrap());
            bind.bind_instanced_geometry_data(&mut instance_buffer);
            let color = match &group.color_ref {
                ColorReference::Material(m) => Vector4::from(&m.color),
                _ => Vector4::zero(),
            };
            bind.bind_non_instanced_color_data(&color);
            
            unsafe {
                gl.draw_arrays_instanced(
                    glow::TRIANGLES,
                    indices.start as i32,
                    indices.span as i32,
                    instance_buffer.count as i32
                );
            }
        }


    }

    pub fn render_display_list(
        &self, parts: &HashMap<PartAlias, Part<GL>>, display_list: &mut DisplayList<GL>
    ) {
        let gl = &self.gl;

        // Render transparent objects first
        unsafe {
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
        }

        for (alias, mut object) in display_list.map.iter_mut() {
            let part = match parts.get(&alias) {
                Some(e) => e,
                None => continue,
            };

            self.render_instance_mesh(&part, &mut object, true);
        }

        // And opaque objects later
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
        }

        for (alias, mut object) in display_list.map.iter_mut() {
            let part = match parts.get(&alias) {
                Some(e) => e,
                None => continue,
            };

            self.render_instance_mesh(&part, &mut object, false);
        }
    }

}
