use std::{
    rc::Rc,
    vec::Vec,
};

use cgmath::{
    SquareMatrix,
    prelude::*
};
use glow::HasContext;
use ldraw::{
    Matrix3, Matrix4, Vector4,
};

use crate::{
    truncate_matrix4,
    shader::{
        Bindable, EdgeProgram, ProgramKind, ProgramManager, ShadedProgram
    },
};

pub struct ProjectionData {
    pub projection: Matrix4,
    pub model_view: Vec<Matrix4>,
    pub view_matrix: Matrix4,
    pub normal_matrix: Matrix3,
}

impl Default for ProjectionData {
    fn default() -> Self {
        ProjectionData {
            projection: Matrix4::identity(),
            model_view: vec![Matrix4::identity()],
            view_matrix: Matrix4::identity(),
            normal_matrix: Matrix3::identity(),
        }
    }
}

impl ProjectionData {
    pub fn update_normal_matrix(&mut self) {
        self.normal_matrix = truncate_matrix4(
            (self.view_matrix * self.model_view.last().unwrap()).invert().unwrap_or(Matrix4::identity()).transpose()
        )
    }

    pub fn derive_normal_matrix(&self, m: &Matrix4) -> Matrix3 {
        truncate_matrix4(
            (m * self.model_view.last().unwrap()).invert().unwrap_or(Matrix4::identity()).transpose()
        )
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

pub struct ShadingData {
    pub light_color: Vector4,
    pub light_direction: Vector4,
}

impl Default for ShadingData {
    fn default() -> Self {
        ShadingData {
            light_color: Vector4::new(1.0, 1.0, 1.0, 1.0),
            light_direction: Vector4::new(1.0, 1.0, 1.0, 1.0),
        }
    }
}

pub struct RenderingContext<'a, GL: HasContext> {
    gl: Rc<GL>,

    pub program_manager: ProgramManager<GL>,
    pub bound: Option<ProgramKind<'a, GL>>,

    pub projection_data: ProjectionData,
    pub shading_data: ShadingData,
}


impl<'a, GL: HasContext> RenderingContext<'a, GL> {
    pub fn new(gl: Rc<GL>, program_manager: ProgramManager<GL>) -> Self {
        RenderingContext {
            gl: Rc::clone(&gl),
            program_manager,
            bound: None,
            projection_data: ProjectionData::default(),
            shading_data: ShadingData::default(),
        }
    }

    pub fn bind_solid(&'a mut self, bfc_certified: bool) -> &'a ShadedProgram<GL> {
        if let Some(e) = &self.bound {
            match (e, bfc_certified) {
                (ProgramKind::Solid(p), true) => return p,
                (ProgramKind::SolidFlat(p), false) => return p,
                (_, _) => {
                    e.unbind();
                }
            }
        }

        if bfc_certified {
            self.bound = Some(ProgramKind::Solid(&self.program_manager.solid));
            &self.program_manager.solid.bind()
        } else {
            self.bound = Some(ProgramKind::SolidFlat(&self.program_manager.solid_flat));
            &self.program_manager.solid_flat.bind()
        }
    }

    pub fn bind_edge(&'a mut self) -> &'a EdgeProgram<GL> {
        if let Some(e) = &self.bound {
            if let ProgramKind::Edge(p) = e {
                return p;
            } else {
                e.unbind();
            }
        }

        self.bound = Some(ProgramKind::Edge(&self.program_manager.edge));
        &self.program_manager.edge.bind()
    }
}
