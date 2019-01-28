use std::cell::RefCell;
use std::collections::HashMap;
use std::hash;
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
pub struct PartCache<'a> {
    items: HashMap<NormalizedAlias, Rc<Document<'a>>>,
}

impl<'a> Default for PartCache<'a> {
    fn default() -> PartCache<'a> {
        PartCache {
            items: HashMap::new(),
        }
    }
}

impl<'a> PartCache<'a> {
    pub fn register(&mut self, alias: NormalizedAlias, document: Document<'a>) {
        self.items.insert(alias, Rc::new(document));
    }

    pub fn query(&self, alias: &NormalizedAlias) -> Option<Rc<Document<'a>>> {
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
    Subpart(Rc<Document<'a>>),
    Associated(Rc<Document<'a>>),
}

#[derive(Clone)]
pub struct ResolutionMap<'a, T> {
    directory: &'a PartDirectory<T>,
    cache: &'a RefCell<PartCache<'a>>,
    pub map: HashMap<NormalizedAlias, ResolutionResult<'a, T>>
}

impl<'a, T: Clone> ResolutionMap<'a, T> {
    pub fn new(directory: &'a PartDirectory<T>, cache: &'a RefCell<PartCache<'a>>) -> ResolutionMap<'a, T> {
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

    pub fn resolve(&mut self, document: Rc<Document<'a>>, parent: Option<&'a MultipartDocument<'a>>) {
        for i in document.iter_refs() {
            let alias = i.name.clone();
            
            if self.map.contains_key(&alias) {
                continue;
            }
            
            if let Some(e) = parent {
                if let Some(doc) = e.query(&alias) {
                    self.resolve(Rc::clone(&doc), parent);
                    self.map.insert(alias, ResolutionResult::Subpart(Rc::clone(&doc)));
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

    pub fn update(&mut self, key: &NormalizedAlias, document: &Rc<Document<'a>>) {
        self.resolve(Rc::clone(document), None);
        self.map.insert(key.clone(), ResolutionResult::Associated(Rc::clone(document)));
    }

    pub fn query(&self, elem: &PartReference) -> Option<Rc<Document<'a>>> {
        match self.map.get(&elem.name) {
            Some(e) => match e {
                ResolutionResult::Missing => None,
                ResolutionResult::Pending(_) => None,
                ResolutionResult::Subpart(e) => Some(Rc::clone(e)),
                ResolutionResult::Associated(e) => Some(Rc::clone(e)),
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
