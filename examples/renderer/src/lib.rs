use std::{
    collections::HashMap,
    rc::Rc,
    vec::Vec,
};

use cgmath::{Deg, PerspectiveFov, Point3, Quaternion, Rad, Rotation3, SquareMatrix};
use glow::HasContext;
use ldraw::{
    color::{ColorReference, Material, MaterialRegistry},
    document::MultipartDocument,
    Matrix3, Matrix4, PartAlias, Vector3, Vector4,
};
use ldraw_renderer::{
    display_list::DisplayList,
    error::RendererError,
    part::Part,
    state::RenderingContext,
    shader::{ProgramManager},
};

pub struct App<GL: HasContext> {
    gl: Rc<GL>,

    features: HashMap<PartAlias, Part<GL>>,
    parts: HashMap<PartAlias, Part<GL>>,

    context: RenderingContext<GL>,
    display_list: DisplayList<GL>,

    document: MultipartDocument,
}

impl<GL: HasContext> App<GL> {

    pub fn new(
        gl: Rc<GL>,
        document: MultipartDocument,
        features: HashMap<PartAlias, Part<GL>>,
        parts: HashMap<PartAlias, Part<GL>>,
        program_manager: ProgramManager<GL>
    ) -> Self {
        App {
            gl: Rc::clone(&gl),
            document,
            features,
            parts,
            context: RenderingContext::new(gl, program_manager),
            display_list: DisplayList::new(),
        }
    }

    pub fn set_up(&self) {
        self.context.set_initial_state();
        self.context.upload_shading_data();
    }

    pub fn animate(&mut self, time: f32) {
        self.context.camera.position.x = time.sin() * 500.0;
        self.context.camera.position.z = time.cos() * 500.0;
        self.context.update_camera();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.context.resize(width, height);
    }

    pub fn render(&mut self) {
        let gl = &self.gl;

        self.context.start_render();

        unsafe {
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            self.context.render_display_list(&self.parts, &mut self.display_list);
            gl.flush();
        }
    }

}
