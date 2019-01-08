use std::cell::RefCell;
use std::collections::HashMap;
use std::hash;
use std::rc::Rc;

use crate::document::{Document, MultipartDocument};
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
            kind: self.kind.clone(),
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

impl<T> PartDirectory<T> {
    pub fn new() -> PartDirectory<T> {
        PartDirectory {
            primitives: HashMap::new(),
            parts: HashMap::new(),
        }
    }

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

impl<'a> PartCache<'a> {
    pub fn new() -> PartCache<'a> {
        PartCache {
            items: HashMap::new(),
        }
    }

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
        self.items.retain(|_, v| Rc::strong_count(&v) > 1);
    }
}

#[derive(Clone, Debug)]
pub enum ResolutionResult<'a, T> {
    Missing,
    Pending(PartEntry<T>),
    Subpart(Rc<Document<'a>>),
    Associated(Rc<Document<'a>>),
}

#[derive(Clone, Debug)]
pub struct ResolutionMap<'a, T> {
    directory: &'a PartDirectory<T>,
    cache: &'a RefCell<PartCache<'a>>,
    pub map: HashMap<NormalizedAlias, ResolutionResult<'a, T>>
}

impl<'a, T: Clone> ResolutionMap<'a, T> {
    pub fn new(directory: &'a PartDirectory<T>, cache: &'a RefCell<PartCache<'a>>) -> ResolutionMap<'a, T> {
        ResolutionMap {
            directory: directory,
            cache: cache,
            map: HashMap::new(),
        }
    }

    pub fn pending(&self) -> Vec<(NormalizedAlias, PartEntry<T>)> {
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
            
            match parent {
                Some(e) => {
                    match e.query(&alias) {
                        Some(doc) => {
                            self.resolve(Rc::clone(&doc), parent);
                            self.map.insert(alias, ResolutionResult::Subpart(Rc::clone(&doc)));
                            continue;
                        },
                        None => (),
                    };
                },
                None => (),
            };
            
            match self.cache.borrow().query(&alias) {
                Some(e) => {
                    self.map.insert(alias, ResolutionResult::Associated(Rc::clone(&e)));
                    continue;
                },
                None => (),
            };
            
            match self.directory.query(&alias) {
                Some(e) => {
                    self.map.insert(alias, ResolutionResult::Pending(e.clone()));
                },
                None => {
                    self.map.insert(alias, ResolutionResult::Missing);
                },
            };
        }
    }

    pub fn update(&mut self, key: &NormalizedAlias, document: &Rc<Document<'a>>) {
        self.resolve(Rc::clone(document), None);
        self.map.insert(key.clone(), ResolutionResult::Associated(Rc::clone(document)));
    }

    pub fn commit(&self, document: Rc<Document<'a>>) {
    }
}

#[cfg(target_arch = "wasm32")]
pub use crate::library_wasm::*;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::library_native::*;
