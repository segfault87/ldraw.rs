use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use glow::HasContext;
use ldraw::{
    color::MaterialRegistry,
    PartAlias,
};
use ldraw_ir::model::Model;
use uuid::Uuid;

use crate::{
    display_list::DisplayList,
    part::Part,
    state::RenderingContext,
};

pub struct RenderableModel<GL: HasContext> {
    pub model: Model,
    pub display_list: DisplayList<GL>,
    pub embedded_parts: HashMap<PartAlias, Part<GL>>,

    pub display_target: Option<Uuid>,
    pub exclusion_set: HashSet<Uuid>,
}

impl<GL: HasContext> RenderableModel<GL> {

    pub fn new(model: Model, gl: Rc<GL>, colors: &MaterialRegistry) -> Self {
        let display_list = DisplayList::from_model(Rc::clone(&gl), &model);
        let embedded_parts = model.embedded_parts.iter().map(
            |(alias, part)| (alias.clone(), Part::create(part, Rc::clone(&gl), colors))
        ).collect::<HashMap<_, _>>();

        RenderableModel {
            model,
            embedded_parts,
            display_list,
            
            display_target: None,
            exclusion_set: HashSet::new(),
        }
    }

    fn update_display_list(&mut self) {
        self.display_list.rebuild(&self.model, self.display_target, &self.exclusion_set);
    }

    pub fn set_render_target(&mut self, group_id: Option<Uuid>) {
        self.display_target = group_id;
        self.update_display_list();
    }

    pub fn clear_exclusion_set(&mut self) {
        if self.exclusion_set.len() > 0 {
            self.exclusion_set.clear();
            self.update_display_list();
        }
    }

    pub fn hide(&mut self, object_id: Uuid) {
        self.exclusion_set.insert(object_id);
        self.update_display_list();
    }

    pub fn render(
        &self,
        context: &mut RenderingContext<GL>,
        parts: &HashMap<PartAlias, Part<GL>>,
        translucent: bool,
    ) {
        for (alias, object) in self.display_list.map.iter() {
            let part = match self.embedded_parts.get(alias) {
                Some(e) => e,
                None => match parts.get(alias) {
                    Some(e) => e,
                    None => continue,
                },
            };
            
            context.render_instanced(part, object, translucent);
        }
    }



}
