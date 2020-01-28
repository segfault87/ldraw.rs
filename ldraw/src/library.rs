use std::cell::RefCell;
use std::collections::HashMap;
use std::hash;
use std::ops::Deref;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::document::{Document, MultipartDocument};
use crate::elements::PartReference;
use crate::AliasType;
use crate::NormalizedAlias;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum PartKind {
    Primitive,
    Part,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PartEntry<T> {
    pub kind: PartKind,
    pub locator: T,
}

impl<T> Clone for PartEntry<T>
where
    T: Clone,
{
    fn clone(&self) -> PartEntry<T> {
        PartEntry {
            kind: self.kind,
            locator: self.locator.clone(),
        }
    }
}

impl<T> hash::Hash for PartEntry<T>
where
    T: hash::Hash,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.locator.hash(state)
    }
}

#[derive(Serialize, Deserialize, Debug)]
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
    primitives: HashMap<NormalizedAlias, Rc<Document>>,
    parts: HashMap<NormalizedAlias, Rc<Document>>,
}

#[derive(Copy, Clone, Debug)]
pub enum CacheCollectionStrategy {
    Parts,
    Primitives,
    PartsAndPrimitives,
}

impl Default for PartCache {
    fn default() -> PartCache {
        PartCache {
            parts: HashMap::new(),
            primitives: HashMap::new(),
        }
    }
}

impl Drop for PartCache {
    fn drop(&mut self) {
        self.collect(CacheCollectionStrategy::PartsAndPrimitives);
    }
}

impl PartCache {
    pub fn register(&mut self, kind: PartKind, alias: NormalizedAlias, document: Document) {
        match kind {
            PartKind::Part => self.parts.insert(alias, Rc::new(document)),
            PartKind::Primitive => self.primitives.insert(alias, Rc::new(document)),
        };
    }

    pub fn query(&self, alias: &NormalizedAlias) -> Option<Rc<Document>> {
        match self.parts.get(alias) {
            Some(part) => Some(Rc::clone(&part)),
            None => match self.primitives.get(alias) {
                Some(prim) => Some(Rc::clone(&prim)),
                None => None,
            },
        }
    }

    fn collect_round(&mut self, collection_strategy: CacheCollectionStrategy) -> usize {
        let prev_size = self.parts.len() + self.primitives.len();
        match collection_strategy {
            CacheCollectionStrategy::Parts => {
                self.parts
                    .retain(|_, v| Rc::strong_count(&v) > 1 || Rc::weak_count(&v) > 0);
            }
            CacheCollectionStrategy::Primitives => {
                self.primitives
                    .retain(|_, v| Rc::strong_count(&v) > 1 || Rc::weak_count(&v) > 0);
            }
            CacheCollectionStrategy::PartsAndPrimitives => {
                self.parts
                    .retain(|_, v| Rc::strong_count(&v) > 1 || Rc::weak_count(&v) > 0);
                self.primitives
                    .retain(|_, v| Rc::strong_count(&v) > 1 || Rc::weak_count(&v) > 0);
            }
        };
        prev_size - self.parts.len() - self.primitives.len()
    }

    pub fn collect(&mut self, collection_strategy: CacheCollectionStrategy) -> usize {
        let mut total_collected = 0;
        loop {
            let collected = self.collect_round(collection_strategy);
            if collected == 0 {
                break;
            }
            total_collected += collected;
        }
        total_collected
    }
}

#[derive(Clone, Debug)]
pub enum ResolutionResult<'a, T> {
    Missing,
    Pending(PartEntry<T>),
    Subpart(&'a Document),
    Associated(Rc<Document>),
}

#[derive(Clone, Debug)]
pub struct ResolutionMap<'a, T> {
    directory: Rc<RefCell<PartDirectory<T>>>,
    cache: Rc<RefCell<PartCache>>,
    pub map: HashMap<NormalizedAlias, ResolutionResult<'a, T>>,
}

impl<'a, 'b, T: Clone> ResolutionMap<'a, T> {
    pub fn new(
        directory: Rc<RefCell<PartDirectory<T>>>,
        cache: Rc<RefCell<PartCache>>,
    ) -> ResolutionMap<'a, T> {
        ResolutionMap {
            directory,
            cache,
            map: HashMap::new(),
        }
    }

    pub fn get_pending(&'b self) -> impl Iterator<Item = (&'b NormalizedAlias, &'b PartEntry<T>)> {
        self.map.iter().filter_map(|(key, value)| match value {
            ResolutionResult::Pending(a) => Some((key, a)),
            _ => None,
        })
    }

    pub fn resolve<D: Deref<Target = Document>>(
        &mut self,
        document: &D,
        parent: Option<&'a MultipartDocument>,
    ) {
        for i in document.iter_refs() {
            let name = &i.name;

            if self.map.contains_key(name) {
                continue;
            }

            if let Some(e) = parent {
                if let Some(doc) = e.subparts.get(name) {
                    self.map
                        .insert(name.clone(), ResolutionResult::Subpart(&doc));
                    self.resolve(&doc, parent);
                    continue;
                }
            }

            let cached = self.cache.borrow().query(name);
            if let Some(e) = cached {
                self.map
                    .insert(name.clone(), ResolutionResult::Associated(Rc::clone(&e)));
                self.resolve(&e, None);
                continue;
            }

            if let Some(e) = self.directory.borrow().query(name) {
                self.map
                    .insert(name.clone(), ResolutionResult::Pending(e.clone()));
            } else {
                self.map.insert(name.clone(), ResolutionResult::Missing);
            }
        }
    }

    pub fn update(&mut self, key: &NormalizedAlias, document: Rc<Document>) {
        self.resolve(&Rc::clone(&document), None);
        self.map.insert(
            key.clone(),
            ResolutionResult::Associated(Rc::clone(&document)),
        );
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

#[cfg(not(target_arch = "wasm32"))]
pub use crate::library_native::*;
