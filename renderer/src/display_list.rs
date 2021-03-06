use std::{
    collections::hash_map::{Entry, HashMap},
    rc::Rc,
    vec::Vec,
};

use ldraw::{
    color::Material,
    {Matrix3, Matrix4, NormalizedAlias, Vector4},
};

use crate::{
    geometry::{OpenGlBakedModel, IndexBound},
    scene::{ProjectionParams, ShadingParams},
    shader::{Bindable, ProgramManager},
    utils::cast_as_bytes,
    GL,
};

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct InstanceGroup {
    pub part_ref: NormalizedAlias,
    pub bfc: bool,
    pub semitransparent: bool,
    pub index_bound: IndexBound,
}

pub struct DisplayItem<T> where T: GL {
    gl: Rc<T>,
    group: InstanceGroup,
    model: Rc<OpenGlBakedModel<T>>,
    pub length: usize,
    mesh_data: Vec<f32>,
    mesh_buffer: Option<T::Buffer>,
    edge_data: Vec<f32>,
    edge_buffer: Option<T::Buffer>,
    needs_update: bool,
}

pub const INSTANCE_BUFFER_MESH_SIZE: usize = 16 + 9 + 4;
pub const INSTANCE_BUFFER_EDGE_SIZE: usize = 16 + 4 + 4;

impl<T> DisplayItem<T> where T: GL {

    pub fn append(&mut self, projection_params: &ProjectionParams, matrix: &Matrix4, material: &Material) -> usize {
        self.mesh_data.extend(AsRef::<[f32; 16]>::as_ref(matrix));
        self.mesh_data.extend(AsRef::<[f32; 9]>::as_ref(&projection_params.calculate_normal_matrix_with(matrix)));
        self.mesh_data.extend(AsRef::<[f32; 4]>::as_ref(&Vector4::from(&material.color)));
        self.edge_data.extend(AsRef::<[f32; 16]>::as_ref(matrix));
        self.edge_data.extend(AsRef::<[f32; 4]>::as_ref(&Vector4::from(&material.color)));
        self.edge_data.extend(AsRef::<[f32; 4]>::as_ref(&Vector4::from(&material.edge)));
        self.length += 1;
        self.needs_update = true;
        self.length - 1
    }

    pub fn remove(&mut self, index: usize) -> Result<(), ()> {
        if index >= self.length {
            return Err(())
        }

        let mesh_start = index * INSTANCE_BUFFER_MESH_SIZE;
        let edge_start = index * INSTANCE_BUFFER_EDGE_SIZE;
        self.mesh_data.drain(mesh_start..mesh_start + INSTANCE_BUFFER_MESH_SIZE);
        self.edge_data.drain(edge_start..edge_start + INSTANCE_BUFFER_EDGE_SIZE);
        self.length -= 1;
        self.needs_update = true;

        Ok(())
    }

    fn update_gl_buffer(&mut self) {
        if !self.needs_update {
            return;
        }
        
        let gl = &self.gl;

        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, self.mesh_buffer);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER, cast_as_bytes(self.mesh_data.as_ref()), glow::DYNAMIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, self.edge_buffer);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER, cast_as_bytes(self.edge_data.as_ref()), glow::DYNAMIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }

        self.needs_update = false;
    }

    pub fn render_single(&mut self, program_manager: &ProgramManager<T>, projection_params: &ProjectionParams,
                         shading_params: &ShadingParams) {
        let program = if self.group.bfc {
            &program_manager.solid
        } else {
            &program_manager.solid_flat
        };
        program.bind();
        program.bind_uniforms(projection_params, &self.mesh_data[16..25], shading_params,
                              &self.mesh_data[25..29]);
        self.model.buffer.mesh.bind(&program.attrib_position, &program.attrib_normal);

        let gl = &self.gl;
        if self.group.semitransparent {
            unsafe { gl.enable(glow::BLEND); }
        } else {
            unsafe { gl.disable(glow::BLEND); }
        }

        let index = &self.group.index_bound;

        unsafe {
            gl.draw_arrays(glow::TRIANGLES, index.0 as i32, (index.1 - index.0) as i32);
        }

        program.unbind();
    }

    pub fn render_instanced(&mut self, program_manager: &ProgramManager<T>, projection_params: &ProjectionParams,
                            shading_params: &ShadingParams) {

    }

    pub fn render(&mut self, program_manager: &ProgramManager<T>, projection_params: &ProjectionParams,
                  shading_params: &ShadingParams) {
        self.update_gl_buffer();

        if self.length == 0 {
            return;
        }

        match self.length {
            0 => return,
            1 => self.render_single(program_manager, projection_params, shading_params),
            _ => self.render_instanced(program_manager, projection_params, shading_params),
        };
    }

}

impl<T> Drop for DisplayItem<T> where T: GL {

    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            if let Some(e) = self.mesh_buffer {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.edge_buffer {
                gl.delete_buffer(e);
            }
        }
    }
    
}

pub struct DisplayList<T>(HashMap<InstanceGroup, DisplayItem<T>>) where T: GL;

impl<T> DisplayList<T> where T: GL {

    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn query<'a>(&'a mut self, gl: Rc<T>, part_ref: &NormalizedAlias, model: Rc<OpenGlBakedModel<T>>,
                     bfc: bool, semitransparent: bool, index_bound: &IndexBound) -> &'a mut DisplayItem<T> {
        let group = InstanceGroup {
            part_ref: part_ref.clone(),
            bfc,
            semitransparent,
            index_bound: index_bound.clone(),
        };
        self.0.entry(group).or_insert_with(|| {
            let gl_ = &gl;

            let mesh_buffer = unsafe {
                gl_.create_buffer().ok()
            };
            let edge_buffer = unsafe {
                gl_.create_buffer().ok()
            };
            
            DisplayItem {
                gl: gl,
                group: InstanceGroup {
                    part_ref: part_ref.clone(),
                    bfc,
                    semitransparent,
                    index_bound: index_bound.clone(),
                },
                model: Rc::clone(&model),
                length: 0,
                mesh_data: Vec::new(),
                mesh_buffer,
                edge_data: Vec::new(),
                edge_buffer,
                needs_update: false,
            }
        })
    }
    
}


