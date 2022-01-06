use std::{
    collections::HashMap,
    ops::Deref,
    pin::Pin,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use futures::future::{Future, join_all};
use serde::{Deserialize, Serialize};

use crate::{
    color::MaterialRegistry,
    document::{Document, MultipartDocument},
    error::ResolutionError,
    PartAlias,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash)]
pub enum PartKind {
    Primitive,
    Part,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash)]
pub enum FileLocation {
    Library(PartKind),
    Local
}

#[async_trait]
pub trait FileLoader {
    async fn load_materials(&self) -> Result<MaterialRegistry, ResolutionError>;
    async fn load(&self, materials: &MaterialRegistry, alias: PartAlias, local: bool) -> Result<(FileLocation, MultipartDocument), ResolutionError>;
}

#[derive(Debug, Default)]
pub struct PartCache {
    primitives: HashMap<PartAlias, Arc<MultipartDocument>>,
    parts: HashMap<PartAlias, Arc<MultipartDocument>>,
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
    pub fn register(&mut self, kind: PartKind, alias: PartAlias, document: Arc<MultipartDocument>) {
        match kind {
            PartKind::Part => self.parts.insert(alias, document),
            PartKind::Primitive => self.primitives.insert(alias, document),
        };
    }

    pub fn query(&self, alias: &PartAlias) -> Option<Arc<MultipartDocument>> {
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

#[derive(Debug, Default)]
struct TransientDocumentCache {
    documents: HashMap<PartAlias, Arc<MultipartDocument>>,
}

impl TransientDocumentCache {
    pub fn register(&mut self, alias: PartAlias, document: Arc<MultipartDocument>) {
        self.documents.insert(alias, document);
    }

    pub fn query(&self, alias: &PartAlias) -> Option<Arc<MultipartDocument>> {
        self.documents.get(alias).map(Arc::clone)
    }
}

#[derive(Clone, Debug)]
pub enum ResolutionState<'a> {
    Missing,
    Subpart(&'a Document),
    Associated(Arc<MultipartDocument>),
}

#[derive(Debug)]
struct DependencyResolver<'a, 'b> {
    materials: &'b MaterialRegistry,
    cache: Arc<RwLock<PartCache>>,
    local_cache: TransientDocumentCache,

    pub map: HashMap<PartAlias, ResolutionState<'a>>,
    pub local_map: HashMap<PartAlias, ResolutionState<'a>>,
}

impl<'a, 'b> DependencyResolver<'a, 'b> {
    pub fn new(
        materials: &'b MaterialRegistry,
        cache: Arc<RwLock<PartCache>>,
    ) -> DependencyResolver<'a, 'b> {
        DependencyResolver {
            materials,
            cache,
            local_cache: TransientDocumentCache::default(),
            map: HashMap::new(),
            local_map: HashMap::new(),
        }
    }

    pub fn contains_state(&self, alias: &PartAlias, local: bool) -> bool {
        if local {
            self.local_map.contains_key(alias)
        } else {
            self.map.contains_key(alias)
        }
    }

    pub fn put_state(&mut self, alias: PartAlias, local: bool, state: ResolutionState<'a>) {
        if local {
            self.local_map.insert(alias, state);
        } else {
            self.map.insert(alias, state);
        }
    }

    pub fn resolve<'x, L: FileLoader, D: 'x + Deref<Target = Document>>(
        &'x mut self,
        loader: &'x L,
        document: D,
        parent: Option<&'a MultipartDocument>,
        local: bool
    ) -> Pin<Box<dyn Future<Output = ()> + 'x>> {
        Box::pin(async move {
            let mut pending = vec![];
            let mut pending_futs = vec![];

            for r in document.iter_refs() {
                let alias = &r.name;

                if self.contains_state(&alias, local) {
                    continue;
                }

                if let Some(e) = parent {
                    if let Some(subpart) = e.subparts.get(alias) {
                        self.put_state(alias.clone(), local, ResolutionState::Subpart(subpart));
                        self.resolve(loader, subpart, parent, local).await;
                        continue;
                    }
                }

                if local {
                    if let Some(cached) = self.local_cache.query(alias) {
                        self.put_state(alias.clone(), true, ResolutionState::Associated(Arc::clone(&cached)));
                        continue;
                    }
                }

                let cached = self.cache.read().unwrap().query(alias);
                if let Some(cached) = cached {
                    self.put_state(alias.clone(), false, ResolutionState::Associated(Arc::clone(&cached)));
                    continue;
                }

                if !pending.contains(alias) {
                    pending.push(alias.clone());
                    pending_futs.push(loader.load(self.materials, alias.clone(), local));
                }
            }

            let result = join_all(pending_futs).await;
            for (alias, result) in pending.iter().zip(result.into_iter()) {
                let mut local = local;
                let state = match result {
                    Ok((location, document)) => {
                        let document = Arc::new(document);
                        match location {
                            FileLocation::Library(kind) => {
                                local = false;
                                self.cache.write().unwrap().register(kind, alias.clone(), Arc::clone(&document));
                            },
                            FileLocation::Local => {
                                self.local_cache.register(alias.clone(), Arc::clone(&document));
                            },
                        };

                        ResolutionState::Associated(document)
                    },
                    Err(_) => ResolutionState::Missing,
                };

                if !self.contains_state(&alias, local) {
                    self.put_state(alias.clone(), local, state.clone());
                    if let ResolutionState::Associated(document) = state {
                        self.resolve(loader, &document.body, None, local).await;
                    }
                }
                
            }
        })
    }
}

#[derive(Debug, Default)]
pub struct ResolutionResult {
    library_entries: HashMap<PartAlias, Arc<MultipartDocument>>,
    local_entries: HashMap<PartAlias, Arc<MultipartDocument>>,
}

impl ResolutionResult {
    pub fn query(&self, alias: &PartAlias, local: bool) -> Option<(Arc<MultipartDocument>, bool)> {
        if local {
            let local_entry = self.local_entries.get(alias);
            if let Some(e) = local_entry {
                return Some((Arc::clone(&e), true));
            }
        }
        self.library_entries.get(alias).map(|e| (Arc::clone(e), false))
    }
}

pub async fn resolve_dependencies<F, L>(
    cache: Arc<RwLock<PartCache>>,
    materials: &MaterialRegistry,
    loader: &L,
    document: &MultipartDocument,
    on_update: F
) -> ResolutionResult
where
    F: Fn(PartAlias, Result<(), ResolutionError>),
    L: FileLoader {
    let mut resolver = DependencyResolver::new(materials, cache);
    resolver.resolve(loader, &document.body, Some(document), true).await;

    ResolutionResult {
        library_entries: resolver.map.into_iter().filter_map(|(k, v)|
            match v {
                ResolutionState::Associated(e) => Some((k, e)),
                _ => None,
            }
        ).collect::<HashMap<_, _>>(),
        local_entries: resolver.local_map.into_iter().filter_map(|(k, v)|
            match v {
                ResolutionState::Associated(e) => Some((k, e)),
                _ => None,
            }
        ).collect::<HashMap<_, _>>(),
    }
}
