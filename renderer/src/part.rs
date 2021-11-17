use std::{collections::HashMap, rc::Rc};

use glow::HasContext;
use ldraw::Vector3;
use ldraw_ir::{
    geometry::BoundingBox3,
    part::{
        EdgeBufferBuilder, FeatureMap, MeshBufferBuilder, OptionalEdgeBufferBuilder,
        PartBufferBuilder, PartBuilder, SubpartIndex,
    },
    MeshGroup,
};

use crate::utils::cast_as_bytes;

#[derive(Debug)]
pub struct MeshBuffer<GL: HasContext> {
    gl: Rc<GL>,

    pub array: Option<GL::VertexArray>,
    pub buffer_vertices: Option<GL::Buffer>,
    pub buffer_normals: Option<GL::Buffer>,
    pub length: usize,
}

impl<GL: HasContext> MeshBuffer<GL> {
    pub fn create(builder: &MeshBufferBuilder, gl: Rc<GL>) -> Self {
        let array: Option<GL::VertexArray>;
        let buffer_vertices: Option<GL::Buffer>;
        let buffer_normals: Option<GL::Buffer>;
        unsafe {
            array = gl.create_vertex_array().ok();
            buffer_vertices = gl.create_buffer().ok();
            buffer_normals = gl.create_buffer().ok();
            gl.bind_vertex_array(array);
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.vertices.as_ref()),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_normals);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.normals.as_ref()),
                glow::STATIC_DRAW,
            );
        }

        MeshBuffer {
            gl: Rc::clone(&gl),
            array,
            buffer_vertices,
            buffer_normals,
            length: builder.len(),
        }
    }

    pub fn bind(&self, location_position: &Option<u32>, location_normals: &Option<u32>) {
        let gl = &self.gl;

        unsafe {
            gl.bind_vertex_array(self.array);
        }

        if let Some(e) = location_position {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_vertices);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
        if let Some(e) = location_normals {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_normals);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
    }
}

impl<GL: HasContext> Drop for MeshBuffer<GL> {
    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            if let Some(e) = self.array {
                gl.delete_vertex_array(e);
            }
            if let Some(e) = self.buffer_vertices {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_normals {
                gl.delete_buffer(e);
            }
        }
    }
}

#[derive(Debug)]
pub struct EdgeBuffer<GL: HasContext> {
    gl: Rc<GL>,

    pub array: Option<GL::VertexArray>,
    pub buffer_vertices: Option<GL::Buffer>,
    pub buffer_colors: Option<GL::Buffer>,
    pub length: usize,
}

impl<GL: HasContext> EdgeBuffer<GL> {
    pub fn create(builder: &EdgeBufferBuilder, gl: Rc<GL>) -> Self {
        let array: Option<GL::VertexArray>;
        let buffer_vertices: Option<GL::Buffer>;
        let buffer_colors: Option<GL::Buffer>;
        unsafe {
            array = gl.create_vertex_array().ok();
            buffer_vertices = gl.create_buffer().ok();
            buffer_colors = gl.create_buffer().ok();
            gl.bind_vertex_array(array);
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.vertices.as_ref()),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_colors);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.colors.as_ref()),
                glow::STATIC_DRAW,
            );
        }

        EdgeBuffer {
            gl: Rc::clone(&gl),
            array,
            buffer_vertices,
            buffer_colors,
            length: builder.len(),
        }
    }

    pub fn bind(&self, location_position: &Option<u32>, location_colors: &Option<u32>) {
        let gl = &self.gl;

        unsafe {
            gl.bind_vertex_array(self.array);
        }

        if let Some(e) = location_position {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_vertices);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
        if let Some(e) = location_colors {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_colors);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
    }
}

impl<GL: HasContext> Drop for EdgeBuffer<GL> {
    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            if let Some(e) = self.array {
                gl.delete_vertex_array(e);
            }
            if let Some(e) = self.buffer_vertices {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_colors {
                gl.delete_buffer(e);
            }
        }
    }
}

#[derive(Debug)]
pub struct OptionalEdgeBuffer<GL: HasContext> {
    gl: Rc<GL>,

    pub array: Option<GL::VertexArray>,
    pub buffer_vertices: Option<GL::Buffer>,
    pub buffer_controls_1: Option<GL::Buffer>,
    pub buffer_controls_2: Option<GL::Buffer>,
    pub buffer_directions: Option<GL::Buffer>,
    pub buffer_colors: Option<GL::Buffer>,
    pub length: usize,
}

impl<GL: HasContext> OptionalEdgeBuffer<GL> {
    pub fn create(builder: &OptionalEdgeBufferBuilder, gl: Rc<GL>) -> Self {
        let array: Option<GL::VertexArray>;
        let buffer_vertices: Option<GL::Buffer>;
        let buffer_controls_1: Option<GL::Buffer>;
        let buffer_controls_2: Option<GL::Buffer>;
        let buffer_directions: Option<GL::Buffer>;
        let buffer_colors: Option<GL::Buffer>;
        unsafe {
            array = gl.create_vertex_array().ok();
            buffer_vertices = gl.create_buffer().ok();
            buffer_controls_1 = gl.create_buffer().ok();
            buffer_controls_2 = gl.create_buffer().ok();
            buffer_directions = gl.create_buffer().ok();
            buffer_colors = gl.create_buffer().ok();
            gl.bind_vertex_array(array);
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.vertices.as_ref()),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_controls_1);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.controls_1.as_ref()),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_controls_2);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.controls_2.as_ref()),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_directions);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.direction.as_ref()),
                glow::STATIC_DRAW,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_colors);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(builder.colors.as_ref()),
                glow::STATIC_DRAW,
            );
        }

        OptionalEdgeBuffer {
            gl: Rc::clone(&gl),
            array,
            buffer_vertices,
            buffer_controls_1,
            buffer_controls_2,
            buffer_directions,
            buffer_colors,
            length: builder.len(),
        }
    }

    pub fn bind(
        &self,
        location_position: &Option<u32>,
        location_colors: &Option<u32>,
        location_controls_1: &Option<u32>,
        location_controls_2: &Option<u32>,
        location_direction: &Option<u32>,
    ) {
        let gl = &self.gl;

        unsafe {
            gl.bind_vertex_array(self.array);
        }

        if let Some(e) = location_position {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_vertices);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
        if let Some(e) = location_colors {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_colors);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
        if let Some(e) = location_controls_1 {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_controls_1);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
        if let Some(e) = location_controls_2 {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_controls_2);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
        if let Some(e) = location_direction {
            unsafe {
                gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer_directions);
                gl.vertex_attrib_pointer_f32(*e, 3, glow::FLOAT, false, 0, 0);
            }
        }
    }
}

impl<GL: HasContext> Drop for OptionalEdgeBuffer<GL> {
    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            if let Some(e) = self.array {
                gl.delete_vertex_array(e);
            }
            if let Some(e) = self.buffer_vertices {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_controls_1 {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_controls_2 {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_directions {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_colors {
                gl.delete_buffer(e);
            }
        }
    }
}

#[derive(Debug)]
pub struct PartBuffer<GL>
where
    GL: HasContext,
{
    pub uncolored_index: Option<SubpartIndex>,
    pub uncolored_without_bfc_index: Option<SubpartIndex>,
    pub opaque_indices: HashMap<MeshGroup, SubpartIndex>,
    pub translucent_indices: HashMap<MeshGroup, SubpartIndex>,

    pub mesh: Option<MeshBuffer<GL>>,
    pub edges: Option<EdgeBuffer<GL>>,
    pub optional_edges: Option<OptionalEdgeBuffer<GL>>,
}

impl<GL: HasContext> PartBuffer<GL> {
    pub fn create(builder: &PartBufferBuilder, gl: Rc<GL>) -> Self {
        let mut merged = MeshBufferBuilder::default();
        let mut opaque = HashMap::new();
        let mut translucent = HashMap::new();
        let mut ptr: usize = 0;

        let uncolored_index = if builder.uncolored_mesh.is_empty() {
            None
        } else {
            merged.vertices.extend(&builder.uncolored_mesh.vertices);
            merged.normals.extend(&builder.uncolored_mesh.normals);
            let cur = ptr;
            ptr += builder.uncolored_mesh.len();

            Some(SubpartIndex {
                start: cur,
                span: builder.uncolored_mesh.len(),
            })
        };

        let uncolored_without_bfc_index = if builder.uncolored_without_bfc_mesh.is_empty() {
            None
        } else {
            merged
                .vertices
                .extend(&builder.uncolored_without_bfc_mesh.vertices);
            merged
                .normals
                .extend(&builder.uncolored_without_bfc_mesh.normals);
            let cur = ptr;
            ptr += builder.uncolored_without_bfc_mesh.len();

            Some(SubpartIndex {
                start: cur,
                span: builder.uncolored_without_bfc_mesh.len(),
            })
        };

        for (group, mesh) in builder.opaque_meshes.iter() {
            merged.vertices.extend(&mesh.vertices);
            merged.normals.extend(&mesh.normals);
            let cur = ptr;
            ptr += mesh.len();

            opaque.insert(
                group.clone(),
                SubpartIndex {
                    start: cur,
                    span: mesh.len(),
                },
            );
        }

        for (group, mesh) in builder.translucent_meshes.iter() {
            merged.vertices.extend(&mesh.vertices);
            merged.normals.extend(&mesh.normals);
            let cur = ptr;
            ptr += mesh.len();

            translucent.insert(
                group.clone(),
                SubpartIndex {
                    start: cur,
                    span: mesh.len(),
                },
            );
        }

        let mesh = if !merged.is_empty() {
            Some(MeshBuffer::create(&merged, Rc::clone(&gl)))
        } else {
            None
        };
        let edges = if !builder.edges.is_empty() {
            Some(EdgeBuffer::create(&builder.edges, Rc::clone(&gl)))
        } else {
            None
        };
        let optional_edges = if !builder.optional_edges.is_empty() {
            Some(OptionalEdgeBuffer::create(
                &builder.optional_edges,
                Rc::clone(&gl),
            ))
        } else {
            None
        };

        PartBuffer {
            uncolored_index,
            uncolored_without_bfc_index,
            opaque_indices: opaque,
            translucent_indices: translucent,
            mesh,
            edges,
            optional_edges,
        }
    }

    pub fn has_opaque_parts(&self) -> bool {
        !self.opaque_indices.is_empty()
    }

    pub fn has_translucent_parts(&self) -> bool {
        !self.translucent_indices.is_empty()
    }
}

#[derive(Debug)]
pub struct Part<GL: HasContext> {
    pub part: PartBuffer<GL>,
    pub features: FeatureMap,
    pub bounding_box: BoundingBox3,
    pub rotation_center: Vector3,
}

impl<GL: HasContext> Part<GL> {
    pub fn create(builder: &PartBuilder, gl: Rc<GL>) -> Self {
        Part {
            part: PartBuffer::create(&builder.part_builder, Rc::clone(&gl)),
            features: builder.features.clone(),
            bounding_box: builder.bounding_box.clone(),
            rotation_center: builder.rotation_center,
        }
    }
}
