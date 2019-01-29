use std::cell::RefCell;
use std::collections::HashMap;
use std::hash;
use std::ops::Deref;
use std::rc::Rc;

use crate::document::{Document, MultipartDocument};
use crate::elements::PartReference;
use crate::NormalizedAlias;

#[derive(Clone, Copy, Debug)]
pub enum PartKind {
    Primitive,
    Part,
}

#[derive(Debug)]
pub struct PartEntry<T> {
    pub kind: PartKind,
    pub locator: T,
}

impl<T> Clone for PartEntry<T> where T: Clone {
    fn clone(&self) -> PartEntry<T> {
        PartEntry {
            kind: self.kind,
            locator: self.locator.clone(),
        }
    }
}

impl<T> hash::Hash for PartEntry<T> where T: hash::Hash {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.locator.hash(state)
    }
}

#[derive(Debug)]
pub struct PartDirectory<T> {
    pub primitives: HashMap<NormalizedAlias, PartEntry<T>>,
    pub parts: HashMap<NormalizedAlias, PartEntry<T>>,
}

impl<T> Default for PartDirectory<T> {
    fn default() -> PartDirectory<T> {
        PartDirectory {
            primitives: HashMap::new(),
            parts: HashMap::new(),
        }
    }
}

impl<T> PartDirectory<T> {
    pub fn add(&mut self, key: NormalizedAlias, entry: PartEntry<T>) {
        match entry.kind {
            PartKind::Primitive => self.primitives.insert(key, entry),
            PartKind::Part => self.parts.insert(key, entry),
        };
    }

    pub fn query(&self, key: &NormalizedAlias) -> Option<&PartEntry<T>> {
        match self.parts.get(key) {
            Some(v) => Some(v),
            None => match self.primitives.get(&key) {
                Some(v) => Some(v),
                None => None,
            },
        }
    }
}

#[derive(Debug)]
pub struct PartCache {
    items: HashMap<NormalizedAlias, Rc<Document>>,
}

impl Default for PartCache {
    fn default() -> PartCache {
        PartCache {
            items: HashMap::new(),
        }
    }
}

impl PartCache {
    pub fn register(&mut self, alias: NormalizedAlias, document: Document) {
        self.items.insert(alias, Rc::new(document));
    }

    pub fn query(&self, alias: &NormalizedAlias) -> Option<Rc<Document>> {
        match self.items.get(alias) {
            Some(e) => Some(Rc::clone(&e)),
            None => None,
        }
    }

    pub fn collect(&mut self) {
        self.items.retain(|_, v| Rc::strong_count(&v) > 1 || Rc::weak_count(&v) > 1);
    }
}

#[derive(Clone)]
pub enum ResolutionResult<'a, T> {
    Missing,
    Pending(PartEntry<T>),
    Subpart(&'a Document),
    Associated(Rc<Document>),
}

#[derive(Clone)]
pub struct ResolutionMap<'a, T> {
    directory: &'a PartDirectory<T>,
    cache: &'a RefCell<PartCache>,
    pub map: HashMap<NormalizedAlias, ResolutionResult<'a, T>>
}

impl<'a, T: Clone> ResolutionMap<'a, T> {
    pub fn new(directory: &'a PartDirectory<T>, cache: &'a RefCell<PartCache>) -> ResolutionMap<'a, T> {
        ResolutionMap {
            directory,
            cache,
            map: HashMap::new(),
        }
    }

    pub fn get_pending(&self) -> Vec<(NormalizedAlias, PartEntry<T>)> {
        self.map.iter().filter_map(|(key, value)| match value {
            ResolutionResult::Pending(a) => Some((key.clone(), a.clone())),
            _ => None,
        }).collect::<Vec<_>>()
    }

    pub fn resolve<D: Deref<Target = Document>>(
        &mut self, document: &D, parent: Option<&'a MultipartDocument>
    ) {
        for i in document.iter_refs() {
            let alias = i.name.clone();
            
            if self.map.contains_key(&alias) {
                continue;
            }
            
            if let Some(e) = parent {
                if let Some(doc) = e.subparts.get(&alias) {
                    self.resolve(&doc, parent);
                    self.map.insert(alias, ResolutionResult::Subpart(&doc));
                    continue;
                }
            }
            
            if let Some(e) = self.cache.borrow().query(&alias) {
                self.map.insert(alias, ResolutionResult::Associated(Rc::clone(&e)));
                continue;
            }
            
            if let Some(e) = self.directory.query(&alias) {
                self.map.insert(alias, ResolutionResult::Pending(e.clone()));
            } else {
                self.map.insert(alias, ResolutionResult::Missing);
            }
        }
    }

    pub fn update(&mut self, key: &NormalizedAlias, document: Rc<Document>) {
        self.resolve(&Rc::clone(&document), None);
        self.map.insert(key.clone(), ResolutionResult::Associated(Rc::clone(&document)));
    }

    pub fn query(&'a self, elem: &PartReference) -> Option<&'a Document> {
        match self.map.get(&elem.name) {
            Some(e) => match e {
                ResolutionResult::Missing => None,
                ResolutionResult::Pending(_) => None,
                ResolutionResult::Subpart(e) => Some(e),
                ResolutionResult::Associated(e) => Some(&e),
            },
            None => None,
        }
    }

    pub fn get(&self, elem: &PartReference) -> Option<&ResolutionResult<T>> {
        self.map.get(&elem.name)
    }
}

#[cfg(target_arch = "wasm32")]
pub use crate::library_wasm::*;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::library_native::*;
