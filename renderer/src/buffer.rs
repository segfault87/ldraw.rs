use std::{
    rc::Rc,
    slice::from_raw_parts,
    vec::Vec,
};

use ldraw::{
    color::ColorReference,
    {Vector3, Vector4}
};
use serde::{Deserialize, Serialize};

use crate::GL;

fn cast_as_bytes<'a>(input: &'a [f32]) -> &'a [u8] {
    unsafe { from_raw_parts(input.as_ptr() as *const u8, input.len() * 4) }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NativeEdgeBuffer {
    pub vertices: Vec<f32>,
    pub colors: Vec<f32>,
}

impl NativeEdgeBuffer {
    pub fn new() -> NativeEdgeBuffer {
        NativeEdgeBuffer {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct NativeMeshBuffer {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
}

impl NativeMeshBuffer {
    pub fn new() -> NativeMeshBuffer {
        NativeMeshBuffer {
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

impl Default for NativeMeshBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct OpenGlMeshBuffer<T> where T: GL {
    gl: Rc<T>,
    
    array: Option<T::VertexArray>,
    buffer_vertices: Option<T::Buffer>,
    buffer_normals: Option<T::Buffer>,
}

impl<T> OpenGlMeshBuffer<T> where T: GL {

    pub fn create(gl: Rc<T>, buffer: &NativeMeshBuffer) -> Self {
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

        OpenGlMeshBuffer {
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

impl<T> Drop for OpenGlMeshBuffer<T> where T: GL {

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
pub struct OpenGlEdgeBuffer<T: GL> {
    gl: Rc<T>,
    
    array: Option<T::VertexArray>,
    buffer_vertices: Option<T::Buffer>,
    buffer_colors: Option<T::Buffer>
}

impl<T> OpenGlEdgeBuffer<T> where T: GL {

    pub fn create(gl: Rc<T>, buffer: &NativeEdgeBuffer) -> Self {
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

        OpenGlEdgeBuffer {
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

impl<T> Drop for OpenGlEdgeBuffer<T> where T: GL {

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

pub trait Buffer {}

#[derive(Debug, Serialize, Deserialize)]
pub struct NativeBuffer {
    pub mesh: NativeMeshBuffer,
    pub edges: NativeEdgeBuffer,
}

impl Buffer for NativeBuffer {}

#[derive(Debug)]
pub struct OpenGlBuffer<T> where T: GL {
    pub mesh: OpenGlMeshBuffer<T>,
    pub edges: OpenGlEdgeBuffer<T>,
}

impl<T> Buffer for OpenGlBuffer<T> where T: GL {}

impl<T> OpenGlBuffer<T> where T: GL {

    pub fn create(gl: Rc<T>, native_buffer: &NativeBuffer) -> OpenGlBuffer<T> {
        OpenGlBuffer {
            mesh: OpenGlMeshBuffer::create(Rc::clone(&gl), &native_buffer.mesh),
            edges: OpenGlEdgeBuffer::create(Rc::clone(&gl), &native_buffer.edges),
        }
    }
    
}

