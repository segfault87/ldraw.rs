use std::{
    collections::hash_map::{Entry, HashMap},
    rc::Rc,
    vec::Vec,
};

use glow::HasContext;
use ldraw::{
    color::{ColorReference, Material},
    Matrix3, Matrix4, PartAlias, Vector4,
};

use crate::{
    part::Part,
    shader::{Bindable, ProgramManager},
    state::{ProjectionData, RenderingContext, ShadingData},
    utils::cast_as_bytes,
};

pub struct DisplayItemBuilder {
    name: PartAlias,
    matrices: Vec<Matrix4>,
    colors: Vec<ColorReference>,
}

impl DisplayItemBuilder {
    pub fn new(name: PartAlias) -> Self {
        DisplayItemBuilder {
            name,
            matrices: vec![],
            colors: vec![],
        }
    }

}

pub struct DisplayItem<GL: HasContext> {
    gl: Rc<GL>,

    pub part: Option<Rc<Part<GL>>>,
    pub count: usize,
    pub matrices: Vec<f32>,
    pub colors: Vec<f32>,
    pub matrices_buffer: GL::Buffer,
    
}

pub const INSTANCE_BUFFER_MESH_SIZE: usize = 16 + 9 + 4;
pub const INSTANCE_BUFFER_EDGE_SIZE: usize = 16 + 4 + 4;

/*impl<GL: HasContext> DisplayItem<GL> {

    pub fn append(&mut self, projection_data: &ProjectionData, matrix: &Matrix4, material: &Material) -> usize {
        self.mesh_data.extend(AsRef::<[f32; 16]>::as_ref(matrix));
        self.mesh_data.extend(AsRef::<[f32; 9]>::as_ref(&projection_data.derive_normal_matrix(matrix)));
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

    pub fn render_single(&mut self, state: &mut RenderingContext<GL>) {
        let gl = &self.gl;
        let program = if self.group.bfc {
            &state.program_manager.solid
        } else {
            &state.program_manager.solid_flat
        };
        program.bind();
        program.bind_uniforms(&state.projection_data, array_ref!(self.mesh_data, 16, 9), &state.shading_data,
                              array_ref!(self.mesh_data, 25, 4));
        self.model.buffer.mesh.bind(&program.attrib_position, &program.attrib_normal);

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

    pub fn render_instanced(&mut self, state: &mut RenderingContext<GL>) {
        let gl = &self.gl;
        let program = if self.group.bfc {
            &state.program_manager.instanced_solid
        } else {
            &state.program_manager.instanced_solid_flat
        };
        program.bind();

        if self.group.semitransparent {
            unsafe { gl.enable(glow::BLEND); }
        } else {
            unsafe { gl.disable(glow::BLEND); }
        }

        let index = &self.group.index_bound;

        program.bind_uniforms(&state.projection_data, &state.shading_data);
        self.model.buffer.mesh.bind(&program.attrib_position, &program.attrib_normal);

        if self.group.semitransparent {
            unsafe { gl.enable(glow::BLEND); }
        } else {
            unsafe { gl.disable(glow::BLEND); }
        }

        let index = &self.group.index_bound;

        unsafe {
            gl.draw_arrays_instanced(glow::TRIANGLES, index.0 as i32, (index.1 - index.0) as i32, self.length as i32);
        }
        
        program.unbind();
    }

    pub fn render(&mut self, state: &mut RenderingContext<GL>) {
        self.update_gl_buffer();

        match self.length {
            0 => return,
            1 => self.render_single(state),
            _ => self.render_instanced(state),
        };
    }

}

impl<GL: HasContext> Drop for DisplayItem<GL> {

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

pub struct DisplayList<GL: HasContext>(HashMap<InstanceGroup, DisplayItem<GL>>);

impl<GL: HasContext> DisplayList<GL> {

    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn query<'a>(&'a mut self, gl: Rc<GL>, part_ref: &PartAlias, model: Rc<BakedPart<GL>>,
                     bfc: bool, semitransparent: bool, index_bound: &IndexBound) -> &'a mut DisplayItem<GL> {
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
                gl: Rc::clone(&gl),
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

pub struct InstanceGroupA<'ft, GL: HasContext> {
    pub part: Option<&'ft BakedPart<GL>>,

    
}*/

