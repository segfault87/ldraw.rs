use std::{
    rc::Rc,
    vec::Vec,
};

use ldraw::{
    color::ColorReference,
    {Vector3, Vector4}
};
use serde::{Deserialize, Serialize};

use crate::{
    utils::cast_as_bytes,
    GL,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EdgeBufferBuilder {
    pub vertices: Vec<f32>,
    pub colors: Vec<f32>,
}

impl EdgeBufferBuilder {
    pub fn new() -> EdgeBufferBuilder {
        EdgeBufferBuilder {
            vertices: Vec::new(),
            colors: Vec::new(),
        }
    }

    pub fn add(&mut self, vec: &Vector3, color: &ColorReference, top: &ColorReference) {
        self.vertices.push(vec.x);
        self.vertices.push(vec.y);
        self.vertices.push(vec.z);

        if color.is_current() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.color.into();
                self.colors.push(mv.x);
                self.colors.push(mv.y);
                self.colors.push(mv.z);
            } else {
                self.colors.push(-1.0);
                self.colors.push(-1.0);
                self.colors.push(-1.0);
            }
        } else if color.is_complement() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.edge.into();
                self.colors.push(mv.x);
                self.colors.push(mv.y);
                self.colors.push(mv.z);
            } else {
                self.colors.push(-2.0);
                self.colors.push(-2.0);
                self.colors.push(-2.0);
            }
        } else if let Some(c) = color.get_material() {
            let mv: Vector4 = c.color.into();
            self.colors.push(mv.x);
            self.colors.push(mv.y);
            self.colors.push(mv.z);
        } else {
            self.colors.push(0.0);
            self.colors.push(0.0);
            self.colors.push(0.0);
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OptionalEdgeBufferBuilder {
    pub vertices: Vec<f32>,
    pub controls: Vec<f32>,
    pub colors: Vec<f32>,
}

impl OptionalEdgeBufferBuilder {
    pub fn new() -> OptionalEdgeBufferBuilder {
        OptionalEdgeBufferBuilder {
            vertices: Vec::new(),
            controls: Vec::new(),
            colors: Vec::new(),
        }
    }

    pub fn add(&mut self, v: &Vector3, c: &Vector3, color: &ColorReference, top: &ColorReference) {
        self.vertices.push(v.x);
        self.vertices.push(v.y);
        self.vertices.push(v.z);

        self.controls.push(c.x);
        self.controls.push(c.y);
        self.controls.push(c.z);

        if color.is_current() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.color.into();
                self.colors.push(mv.x);
                self.colors.push(mv.y);
                self.colors.push(mv.z);
            } else {
                self.colors.push(-1.0);
                self.colors.push(-1.0);
                self.colors.push(-1.0);
            }
        } else if color.is_complement() {
            if let Some(c) = top.get_material() {
                let mv: Vector4 = c.edge.into();
                self.colors.push(mv.x);
                self.colors.push(mv.y);
                self.colors.push(mv.z);
            } else {
                self.colors.push(-2.0);
                self.colors.push(-2.0);
                self.colors.push(-2.0);
            }
        } else if let Some(c) = color.get_material() {
            let mv: Vector4 = c.color.into();
            self.colors.push(mv.x);
            self.colors.push(mv.y);
            self.colors.push(mv.z);
        } else {
            self.colors.push(0.0);
            self.colors.push(0.0);
            self.colors.push(0.0);
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MeshBufferBuilder {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
}

impl MeshBufferBuilder {
    pub fn new() -> MeshBufferBuilder {
        MeshBufferBuilder {
            vertices: Vec::new(),
            normals: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

impl Default for MeshBufferBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct MeshBuffer<T> where T: GL {
    gl: Rc<T>,
    
    array: Option<T::VertexArray>,
    buffer_vertices: Option<T::Buffer>,
    buffer_normals: Option<T::Buffer>,
}

impl<T> MeshBuffer<T> where T: GL {

    pub fn create(gl: Rc<T>, buffer: &MeshBufferBuilder) -> Self {
        let array: Option<T::VertexArray>;
        let buffer_vertices: Option<T::Buffer>;
        let buffer_normals: Option<T::Buffer>;
        unsafe {
            let gl = &gl;
            
            array = gl.create_vertex_array().ok();
            buffer_vertices = gl.create_buffer().ok();
            buffer_normals = gl.create_buffer().ok();
            gl.bind_vertex_array(array);
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(buffer.vertices.as_ref()),
                glow::STATIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_normals);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(buffer.normals.as_ref()),
                glow::STATIC_DRAW
            );
        }

        MeshBuffer {
            gl: Rc::clone(&gl),
            array,
            buffer_vertices,
            buffer_normals,
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

impl<T> Drop for MeshBuffer<T> where T: GL {

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
pub struct EdgeBuffer<T: GL> {
    gl: Rc<T>,
    
    array: Option<T::VertexArray>,
    buffer_vertices: Option<T::Buffer>,
    buffer_colors: Option<T::Buffer>
}

impl<T> EdgeBuffer<T> where T: GL {

    pub fn create(gl: Rc<T>, buffer: &EdgeBufferBuilder) -> Self {
        let array: Option<T::VertexArray>;
        let buffer_vertices: Option<T::Buffer>;
        let buffer_colors: Option<T::Buffer>;
        unsafe {
            let gl = &gl;
            array = gl.create_vertex_array().ok();
            buffer_vertices = gl.create_buffer().ok();
            buffer_colors = gl.create_buffer().ok();
            gl.bind_vertex_array(array);
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(buffer.vertices.as_ref()),
                glow::STATIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_colors);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(buffer.colors.as_ref()),
                glow::STATIC_DRAW
            );
        }

        EdgeBuffer {
            gl: Rc::clone(&gl),
            array,
            buffer_vertices,
            buffer_colors,
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

impl<T> Drop for EdgeBuffer<T> where T: GL {

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
pub struct OptionalEdgeBuffer<T: GL> {
    gl: Rc<T>,
    
    array: Option<T::VertexArray>,
    buffer_vertices: Option<T::Buffer>,
    buffer_controls: Option<T::Buffer>,
    buffer_colors: Option<T::Buffer>
}

impl<T> OptionalEdgeBuffer<T> where T: GL {

    pub fn create(gl: Rc<T>, buffer: &OptionalEdgeBufferBuilder) -> Self {
        let array: Option<T::VertexArray>;
        let buffer_vertices: Option<T::Buffer>;
        let buffer_controls: Option<T::Buffer>;
        let buffer_colors: Option<T::Buffer>;
        unsafe {
            let gl = &gl;
            array = gl.create_vertex_array().ok();
            buffer_vertices = gl.create_buffer().ok();
            buffer_controls = gl.create_buffer().ok();
            buffer_colors = gl.create_buffer().ok();
            gl.bind_vertex_array(array);
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_vertices);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(buffer.vertices.as_ref()),
                glow::STATIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_controls);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(buffer.controls.as_ref()),
                glow::STATIC_DRAW
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, buffer_colors);
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                cast_as_bytes(buffer.colors.as_ref()),
                glow::STATIC_DRAW
            );
        }

        OptionalEdgeBuffer {
            gl: Rc::clone(&gl),
            array,
            buffer_vertices,
            buffer_controls,
            buffer_colors,
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

impl<T> Drop for OptionalEdgeBuffer<T> where T: GL {

    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            if let Some(e) = self.array {
                gl.delete_vertex_array(e);
            }
            if let Some(e) = self.buffer_vertices {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_controls {
                gl.delete_buffer(e);
            }
            if let Some(e) = self.buffer_colors {
                gl.delete_buffer(e);
            }
        }
    }
    
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PartBufferBuilder {
    pub mesh: MeshBufferBuilder,
    pub edges: EdgeBufferBuilder,
    pub optional_edges: OptionalEdgeBufferBuilder,
}

#[derive(Debug)]
pub struct PartBuffer<T> where T: GL {
    pub mesh: MeshBuffer<T>,
    pub edges: EdgeBuffer<T>,
    pub optional_edges: OptionalEdgeBuffer<T>,
}

impl<T> PartBuffer<T> where T: GL {

    pub fn create(gl: Rc<T>, builder: &PartBufferBuilder) -> PartBuffer<T> {
        PartBuffer {
            mesh: MeshBuffer::create(Rc::clone(&gl), &builder.mesh),
            edges: EdgeBuffer::create(Rc::clone(&gl), &builder.edges),
            optional_edges: OptionalEdgeBuffer::create(Rc::clone(&gl), &builder.optional_edges),
        }
    }
    
}

