use std::{
    collections::hash_map::{Entry, HashMap},
    rc::Rc,
    vec::Vec,
};

use cgmath::SquareMatrix;
use glow::HasContext;
use itertools::izip;
use ldraw::{
    color::{ColorReference, Material},
    document::{Document, MultipartDocument},
    Matrix3, Matrix4, PartAlias, Vector4,
};

use crate::{
    part::Part,
    shader::{ProgramManager},
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

pub struct InstanceBuffer<GL: HasContext> {
    gl: Rc<GL>,

    pub count: usize,

    model_view_matrices: Vec<f32>,
    colors: Vec<f32>,
    edge_colors: Vec<f32>,

    pub model_view_matrices_buffer: Option<GL::Buffer>,
    pub color_buffer: Option<GL::Buffer>,
    pub edge_color_buffer: Option<GL::Buffer>,

    modified: bool,
}

impl<GL: HasContext> InstanceBuffer<GL> {
    pub fn new(gl: Rc<GL>) -> Self {
        InstanceBuffer {
            gl,

            count: 0,

            model_view_matrices: vec![],
            colors: vec![],
            edge_colors: vec![],

            model_view_matrices_buffer: None,
            color_buffer: None,
            edge_color_buffer: None,

            modified: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn update_buffer(&mut self, gl: &GL) {
        if !self.modified {
            return;
        }

        if self.model_view_matrices.is_empty() {
            self.model_view_matrices_buffer = None;
        } else {
            if self.model_view_matrices_buffer.is_none() {
                self.model_view_matrices_buffer = unsafe {
                    gl.create_buffer().ok()
                };
            }

            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.model_view_matrices_buffer);
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER, cast_as_bytes(self.model_view_matrices.as_ref()), glow::DYNAMIC_DRAW
                );
            }
        }

        if self.colors.is_empty() {
            self.color_buffer = None;
        } else {
            if self.color_buffer.is_none() {
                self.color_buffer = unsafe {
                    gl.create_buffer().ok()
                };
            }

            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.color_buffer);
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER, cast_as_bytes(self.colors.as_ref()), glow::DYNAMIC_DRAW
                );
            }
        }

        if self.edge_colors.is_empty() {
            self.edge_color_buffer = None;
        } else {
            if self.edge_color_buffer.is_none() {
                self.edge_color_buffer = unsafe {
                    gl.create_buffer().ok()
                };
            }

            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.edge_color_buffer);
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER, cast_as_bytes(self.edge_colors.as_ref()), glow::DYNAMIC_DRAW
                );
            }
        }
    }
}

impl<GL: HasContext> Drop for InstanceBuffer<GL> {
    fn drop(&mut self) {
        let gl = &self.gl;

        unsafe {
            if let Some(b) = self.model_view_matrices_buffer {
                gl.delete_buffer(b);
            }
            if let Some(b) = self.color_buffer {
                gl.delete_buffer(b);
            }
            if let Some(b) = self.edge_color_buffer {
                gl.delete_buffer(b);
            }
        }
    }
}

pub struct DisplayItem<GL: HasContext> {
    pub part: PartAlias,

    pub opaque: InstanceBuffer<GL>,
    pub semitransparent: InstanceBuffer<GL>,
}

impl<GL: HasContext> DisplayItem<GL> {

    pub fn new(gl: Rc<GL>, alias: &PartAlias) -> Self {
        DisplayItem {
            part: alias.clone(),

            opaque: InstanceBuffer::new(Rc::clone(&gl)),
            semitransparent: InstanceBuffer::new(Rc::clone(&gl)),
        }
    }

    /* TODO: This is temporary; should be superseded with sophisticated editor stuffs */
    pub fn update_data(
        &mut self,
        opaque: bool,
        model_view_matrices: &Vec<Matrix4>,
        color_buffer: &Vec<Vector4>,
        edge_color_buffer: &Vec<Vector4>,
    ) {
        let mut mvmr = vec![];
        let mut cr = vec![];
        let mut ecr = vec![];
        for (mvm, c, ec) in izip!(model_view_matrices, color_buffer, edge_color_buffer) {
            mvmr.extend(AsRef::<[f32; 16]>::as_ref(mvm));
            cr.extend(AsRef::<[f32; 4]>::as_ref(c));
            ecr.extend(AsRef::<[f32; 4]>::as_ref(ec));
        }

        let buffer = if opaque {
            &mut self.opaque
        } else {
            &mut self.semitransparent
        };

        buffer.model_view_matrices = mvmr;
        buffer.colors = cr;
        buffer.edge_colors = ecr;
        buffer.count = model_view_matrices.len();
        buffer.modified = true;
    }

    pub fn add(
        &mut self,
        matrix: &Matrix4,
        color: &ColorReference
    ) {
        let material = match color {
            ColorReference::Material(m) => m,
            _ => return,
        };

        let buffer = if material.is_semi_transparent() {
            &mut self.semitransparent
        } else {
            &mut self.opaque
        };

        buffer.model_view_matrices.extend(AsRef::<[f32; 16]>::as_ref(matrix));
        buffer.colors.extend(AsRef::<[f32; 4]>::as_ref(&Vector4::from(&material.color)));
        buffer.edge_colors.extend(AsRef::<[f32; 4]>::as_ref(&Vector4::from(&material.edge)));
        buffer.count += 1;
        buffer.modified = true;
    }
}

pub struct DisplayList<GL: HasContext> {
    pub map: HashMap<PartAlias, DisplayItem<GL>>
}

impl<GL: HasContext> DisplayList<GL> {
    pub fn new() -> Self {
        DisplayList {
            map: HashMap::new()
        }
    }
}

fn build_display_list<'a, GL: HasContext>(
    gl: Rc<GL>,
    display_list: &mut DisplayList<GL>,
    document: &'a Document,
    matrix: Matrix4,
    parent: &'a MultipartDocument
) {
    for e in document.iter_refs() {
        if parent.subparts.contains_key(&e.name) {
            build_display_list(Rc::clone(&gl), display_list, parent.subparts.get(&e.name).unwrap(), matrix * e.matrix, parent);
        } else {
            display_list.add(Rc::clone(&gl), &e.name, &(matrix * e.matrix), &e.color);
        }
    }
}

impl<GL: HasContext> DisplayList<GL> {
    pub fn from_multipart_document(gl: Rc<GL>, document: &MultipartDocument) -> Self {
        let mut display_list = DisplayList::new();

        build_display_list(gl, &mut display_list, &document.body, Matrix4::identity(), &document);

        display_list
    }

    pub fn add(&mut self, gl: Rc<GL>, name: &PartAlias, matrix: &Matrix4, color: &ColorReference) {
        self.map.entry(name.clone()).or_insert_with(|| DisplayItem::new(Rc::clone(&gl), &name)).add(&matrix, &color);
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}
   