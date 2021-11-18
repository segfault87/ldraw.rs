use std::{
    collections::HashMap,
    sync::Arc
};

use ldraw::PartAlias;
use ldraw_ir::part::PartBuilder;

pub struct PartRegistry {
    map: HashMap<PartAlias, Arc<PartBuilder>>,
}

impl PartRegistry {
    pub fn new() -> Self {
        PartRegistry {
            map: HashMap::new()
        }
    }

    pub fn register(&mut self, alias: &PartAlias, part: PartBuilder) {
        self.map.insert(alias.clone(), Arc::new(part));
    }

    pub fn query(&self, alias: &PartAlias) -> Option<Arc<PartBuilder>> {
        self.map.get(alias).map(|e| Arc::clone(&e))
    }
}
