use std::{
    rc::Rc,
    vec::Vec,
};

use cgmath::{
    Deg,
    PerspectiveFov,
    Rad,
    SquareMatrix,
    prelude::*
};
use glow::HasContext;
use ldraw::{
    Matrix4, Vector3, Vector4,
};

use crate::{
    truncate_matrix4,
    shader::{
        ProgramManager
    },
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
    /*pub fn update_normal_matrix(&mut self) {
        self.normal_matrix = truncate_matrix4(
            (self.view_matrix * self.model_view.last().unwrap()).invert().unwrap_or(Matrix4::identity()).transpose()
        )
    }

    pub fn derive_normal_matrix(&self, m: &Matrix4) -> Matrix3 {
        truncate_matrix4(
            (m * self.model_view.last().unwrap()).invert().unwrap_or(Matrix4::identity()).transpose()
        )
    }*/

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

pub struct RenderingContext<GL: HasContext> {
    gl: Rc<GL>,

    pub program_manager: ProgramManager<GL>,
    
    pub projection_data: ProjectionData,
    pub shading_data: ShadingData,
}


impl<GL: HasContext> RenderingContext<GL> {
    pub fn new(gl: Rc<GL>, program_manager: ProgramManager<GL>) -> Self {
        let num_directional_lights = program_manager.num_directional_lights;
        let num_point_lights = program_manager.num_point_lights;
        RenderingContext {
            gl: Rc::clone(&gl),
            program_manager,
            projection_data: ProjectionData::default(),
            shading_data: ShadingData::new(
                num_directional_lights, num_point_lights
            ),
        }
    }

    pub fn update_projection_data(&self) {
        self.program_manager.bind_projection_data(&self.projection_data);
    }

    pub fn update_shading_data(&self) {
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
        let proj = Matrix4::from(PerspectiveFov {
            fovy: Rad::from(Deg(45.0)),
            aspect: width as f32 / height as f32,
            near: 0.1,
            far: 100000.0
        }) * Matrix4::from_translation(Vector3::new(0.0, -100.0, -300.0));

        self.projection_data.update_projection_matrix(&proj);
        unsafe {
            self.gl.viewport(0, 0, width as i32, height as i32);
        }
    }

}
