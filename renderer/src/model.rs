use std::{
    cell::RefCell,
    rc::Rc,
};

use glow::HasContext;
use ldraw_ir::model::Model;

use crate::display_list::DisplayList;

pub struct RenderableModel<GL: HasContext> {
    pub model: Model,
    pub display_list: RefCell<DisplayList<GL>>,
}

impl<GL: HasContext> RenderableModel<GL> {

    pub fn new(gl: Rc<GL>, model: Model) -> Self {
        let display_list = DisplayList::from_model(gl, &model);

        RenderableModel {
            model,
            display_list: RefCell::new(display_list),
        }
    }





}
