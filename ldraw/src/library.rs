use std::{
    collections::{HashMap, HashSet},
    hash,
    ops::Deref,
    sync::{Arc, RwLock},
};

use serde::{Deserialize, Serialize};

use crate::{
    document::{Document, MultipartDocument},
    elements::PartReference,
    AliasType, PartAlias,
};

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
    pub primitives: HashMap<PartAlias, PartEntry<T>>,
    pub parts: HashMap<PartAlias, PartEntry<T>>,
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
    pub fn add(&mut self, key: PartAlias, entry: PartEntry<T>) {
        match entry.kind {
            PartKind::Primitive => self.primitives.insert(key, entry),
            PartKind::Part => self.parts.insert(key, entry),
        };
    }

    pub fn query(&self, key: &PartAlias) -> Option<&PartEntry<T>> {
        match self.parts.get(key) {
            Some(v) => Some(v),
            None => match self.primitives.get(key) {
                Some(v) => Some(v),
                None => None,
            },
        }
    }
}

#[derive(Debug, Default)]
pub struct PartCache {
    primitives: HashMap<PartAlias, Arc<Document>>,
    parts: HashMap<PartAlias, Arc<Document>>,
}

#[derive(Copy, Clone, Debug)]
pub enum CacheCollectionStrategy {
    Parts,
    Primitives,
    PartsAndPrimitives,
}

impl Drop for PartCache {
    fn drop(&mut self) {
        self.collect(CacheCollectionStrategy::PartsAndPrimitives);
    }
}

impl PartCache {
    pub fn register(&mut self, kind: PartKind, alias: PartAlias, document: Document) {
        match kind {
            PartKind::Part => self.parts.insert(alias, Arc::new(document)),
            PartKind::Primitive => self.primitives.insert(alias, Arc::new(document)),
        };
    }

    pub fn query(&self, alias: &PartAlias) -> Option<Arc<Document>> {
        match self.parts.get(alias) {
            Some(part) => Some(Arc::clone(part)),
            None => self.primitives.get(alias).map(Arc::clone),
        }
    }

    fn collect_round(&mut self, collection_strategy: CacheCollectionStrategy) -> usize {
        let prev_size = self.parts.len() + self.primitives.len();
        match collection_strategy {
            CacheCollectionStrategy::Parts => {
                self.parts
                    .retain(|_, v| Arc::strong_count(v) > 1 || Arc::weak_count(v) > 0);
            }
            CacheCollectionStrategy::Primitives => {
                self.primitives
                    .retain(|_, v| Arc::strong_count(v) > 1 || Arc::weak_count(v) > 0);
            }
            CacheCollectionStrategy::PartsAndPrimitives => {
                self.parts
                    .retain(|_, v| Arc::strong_count(v) > 1 || Arc::weak_count(v) > 0);
                self.primitives
                    .retain(|_, v| Arc::strong_count(v) > 1 || Arc::weak_count(v) > 0);
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
    Associated(Arc<Document>),
}

#[derive(Clone, Debug)]
pub struct ResolutionMap<'a, T> {
    directory: Arc<RwLock<PartDirectory<T>>>,
    cache: Arc<RwLock<PartCache>>,
    pub map: HashMap<PartAlias, ResolutionResult<'a, T>>,
}

impl<'a, 'b, T: Clone> ResolutionMap<'a, T> {
    pub fn new(
        directory: Arc<RwLock<PartDirectory<T>>>,
        cache: Arc<RwLock<PartCache>>,
    ) -> ResolutionMap<'a, T> {
        ResolutionMap {
            directory,
            cache,
            map: HashMap::new(),
        }
    }

    pub fn get_pending(&'b self) -> impl Iterator<Item = (&'b PartAlias, &'b PartEntry<T>)> {
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
                        .insert(name.clone(), ResolutionResult::Subpart(doc));
                    self.resolve(&doc, parent);
                    continue;
                }
            }

            let cached = self.cache.read().unwrap().query(name);
            if let Some(e) = cached {
                self.map
                    .insert(name.clone(), ResolutionResult::Associated(Arc::clone(&e)));
                self.resolve(&e, None);
                continue;
            }

            if let Some(e) = self.directory.read().unwrap().query(name) {
                self.map
                    .insert(name.clone(), ResolutionResult::Pending(e.clone()));
            } else {
                self.map.insert(name.clone(), ResolutionResult::Missing);
            }
        }
    }

    pub fn update(&mut self, key: &PartAlias, document: Arc<Document>) {
        self.resolve(&Arc::clone(&document), None);
        self.map.insert(
            key.clone(),
            ResolutionResult::Associated(Arc::clone(&document)),
        );
    }

    pub fn query(&'a self, elem: &PartReference) -> Option<&'a Document> {
        match self.map.get(&elem.name) {
            Some(e) => match e {
                ResolutionResult::Missing => None,
                ResolutionResult::Pending(_) => None,
                ResolutionResult::Subpart(e) => Some(e),
                ResolutionResult::Associated(e) => Some(e),
            },
            None => None,
        }
    }

    pub fn get(&self, elem: &PartReference) -> Option<&ResolutionResult<T>> {
        self.map.get(&elem.name)
    }

    fn traverse_dependencies(&self, document: &Document, list: &mut HashSet<PartAlias>) {
        for part_ref in document.iter_refs() {
            match self.get(part_ref) {
                Some(&ResolutionResult::Subpart(doc)) => {
                    self.traverse_dependencies(doc, list);
                }
                Some(ResolutionResult::Associated(part)) => {
                    if !list.contains(&part_ref.name) {
                        list.insert(part_ref.name.clone());
                    }
                    self.traverse_dependencies(part, list);
                }
                _ => {}
            }
        }
    }

    pub fn list_all_dependencies(&self, document: &Document) -> HashSet<PartAlias> {
        let mut result = HashSet::new();

        self.traverse_dependencies(document, &mut result);

        result
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use crate::library_native::*;
