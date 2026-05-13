use std::{
    collections::{HashMap, HashSet},
    ops::Deref,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize};

use crate::{
    PartAlias,
    color::ColorCatalog,
    document::{Document, MultipartDocument},
    error::ResolutionError,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash)]
pub enum PartKind {
    Primitive,
    Part,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash)]
pub enum FileLocation {
    Library(PartKind),
    Local,
}

#[async_trait(?Send)]
pub trait DocumentLoader<T> {
    async fn load_document(
        &self,
        locator: &T,
        colors: &ColorCatalog,
    ) -> Result<MultipartDocument, ResolutionError>;
}

#[async_trait(?Send)]
pub trait LibraryLoader {
    async fn load_colors(&self) -> Result<ColorCatalog, ResolutionError>;

    async fn load_ref(
        &self,
        alias: PartAlias,
        local: bool,
        colors: &ColorCatalog,
    ) -> Result<(FileLocation, MultipartDocument), ResolutionError>;
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, kind: PartKind, alias: PartAlias, document: Arc<MultipartDocument>) {
        match kind {
            PartKind::Part => self.parts.insert(alias, document),
            PartKind::Primitive => self.primitives.insert(alias, document),
        };
    }

    pub fn query(&self, alias: &PartAlias) -> Option<Arc<MultipartDocument>> {
        match self.parts.get(alias) {
            Some(part) => Some(Arc::clone(part)),
            None => self.primitives.get(alias).cloned(),
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
        self.documents.get(alias).cloned()
    }
}

#[derive(Debug)]
pub enum ResolutionState {
    Missing(ResolutionError),
    Pending,
    Subpart,
    Associated(Arc<MultipartDocument>),
}

struct DependencyResolver<'a, F, L> {
    colors: &'a ColorCatalog,
    cache: Arc<RwLock<PartCache>>,
    local_cache: TransientDocumentCache,
    on_update: &'a F,
    loader: &'a L,

    pub map: HashMap<PartAlias, ResolutionState>,
    pub local_map: HashMap<PartAlias, ResolutionState>,
}

impl<'a, F: Fn(PartAlias, Result<(), &ResolutionError>), L: LibraryLoader>
    DependencyResolver<'a, F, L>
{
    pub fn new(
        colors: &'a ColorCatalog,
        cache: Arc<RwLock<PartCache>>,
        on_update: &'a F,
        loader: &'a L,
    ) -> DependencyResolver<'a, F, L> {
        DependencyResolver {
            colors,
            cache,
            local_cache: TransientDocumentCache::default(),
            on_update,
            loader,
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

    pub fn put_state(&mut self, alias: PartAlias, local: bool, state: ResolutionState) {
        if local {
            self.local_map.insert(alias, state);
        } else {
            self.map.insert(alias, state);
        }
    }

    pub fn clear_state(&mut self, alias: &PartAlias, local: bool) {
        if local {
            self.local_map.remove(alias);
        } else {
            self.map.remove(alias);
        }
    }

    pub fn scan_dependencies(&mut self, document: &Document, local: bool) {
        for r in document.iter_refs() {
            let alias = &r.name;

            if self.contains_state(alias, local) {
                continue;
            }

            if local && let Some(cached) = self.local_cache.query(alias) {
                self.scan_dependencies_with_parent(None, Arc::clone(&cached), true);

                self.put_state(
                    alias.clone(),
                    true,
                    ResolutionState::Associated(Arc::clone(&cached)),
                );
                continue;
            }

            let cached = self.cache.read().unwrap().query(alias);
            if let Some(cached) = cached {
                self.scan_dependencies_with_parent(None, Arc::clone(&cached), false);

                self.put_state(
                    alias.clone(),
                    false,
                    ResolutionState::Associated(Arc::clone(&cached)),
                );
                continue;
            }

            self.put_state(alias.clone(), local, ResolutionState::Pending);
        }
    }

    pub fn scan_dependencies_with_parent<D: Deref<Target = MultipartDocument> + Clone>(
        &mut self,
        alias: Option<&PartAlias>,
        parent: D,
        local: bool,
    ) {
        let document = match alias {
            Some(e) => match parent.subparts.get(e) {
                Some(e) => e,
                None => return,
            },
            None => &parent.body,
        };

        for r in document.iter_refs() {
            let alias = &r.name;

            if self.contains_state(alias, local) {
                continue;
            }

            if parent.subparts.contains_key(alias) {
                self.put_state(alias.clone(), local, ResolutionState::Subpart);
                self.scan_dependencies_with_parent(Some(alias), parent.clone(), local);
                continue;
            }

            if local && let Some(cached) = self.local_cache.query(alias) {
                self.scan_dependencies_with_parent(None, Arc::clone(&cached), true);

                self.put_state(
                    alias.clone(),
                    true,
                    ResolutionState::Associated(Arc::clone(&cached)),
                );
                continue;
            }

            let cached = self.cache.read().unwrap().query(alias);
            if let Some(cached) = cached {
                self.scan_dependencies_with_parent(None, Arc::clone(&cached), false);

                self.put_state(
                    alias.clone(),
                    false,
                    ResolutionState::Associated(Arc::clone(&cached)),
                );
                continue;
            }

            self.put_state(alias.clone(), local, ResolutionState::Pending);
        }
    }

    pub async fn resolve_pending_dependencies(&mut self) -> bool {
        let mut pending = self
            .local_map
            .iter()
            .filter_map(|(k, v)| match v {
                ResolutionState::Pending => Some((k.clone(), true)),
                _ => None,
            })
            .collect::<Vec<_>>();
        pending.extend(self.map.iter().filter_map(|(k, v)| match v {
            ResolutionState::Pending => Some((k.clone(), false)),
            _ => None,
        }));

        if pending.is_empty() {
            return false;
        }

        let futs = pending
            .iter()
            .map(|(alias, local)| self.loader.load_ref(alias.clone(), *local, self.colors))
            .collect::<Vec<_>>();

        let result = join_all(futs).await;

        for ((alias, local), result) in pending.iter().zip(result) {
            let mut local = *local;
            let state = match result {
                Ok((location, document)) => {
                    (self.on_update)(alias.clone(), Ok(()));
                    let document = Arc::new(document);
                    match location {
                        FileLocation::Library(kind) => {
                            if local {
                                self.clear_state(alias, true);
                            }
                            local = false;
                            self.cache.write().unwrap().register(
                                kind,
                                alias.clone(),
                                Arc::clone(&document),
                            );
                        }
                        FileLocation::Local => {
                            self.local_cache
                                .register(alias.clone(), Arc::clone(&document));
                        }
                    };

                    self.scan_dependencies_with_parent(None, Arc::clone(&document), local);

                    ResolutionState::Associated(document)
                }
                Err(err) => {
                    (self.on_update)(alias.clone(), Err(&err));
                    ResolutionState::Missing(err)
                }
            };
            self.put_state(alias.clone(), local, state);
        }

        true
    }
}

#[derive(Debug, Default)]
pub struct ResolutionResult {
    library_entries: HashMap<PartAlias, Arc<MultipartDocument>>,
    local_entries: HashMap<PartAlias, Arc<MultipartDocument>>,
    failures: HashMap<PartAlias, ResolutionError>,
}

impl ResolutionResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up the resolved document for `alias`.
    ///
    /// When `local = true`, local entries are preferred over library entries
    /// (with the same fallback behavior as [`LibraryLoader::load_ref`]). When
    /// `local = false`, only the library entries are searched.
    ///
    /// The returned `bool` indicates whether the hit came from local entries
    /// (`true`) or library entries (`false`).
    pub fn query(&self, alias: &PartAlias, local: bool) -> Option<(Arc<MultipartDocument>, bool)> {
        if local {
            let local_entry = self.local_entries.get(alias);
            if let Some(e) = local_entry {
                return Some((Arc::clone(e), true));
            }
        }
        self.library_entries
            .get(alias)
            .map(|e| (Arc::clone(e), false))
    }

    pub fn list_dependencies(&self) -> HashSet<PartAlias> {
        let mut result = HashSet::new();

        result.extend(self.library_entries.keys().cloned());
        result.extend(self.local_entries.keys().cloned());

        result
    }

    /// Sub-part references that could not be resolved, keyed by alias.
    ///
    /// Entries appear here when [`LibraryLoader::load_ref`] returned an error
    /// for a referenced part. A non-empty `failures()` typically means a
    /// subsequent `bake_part_from_*` call will produce a part with missing
    /// geometry; callers should inspect this before baking.
    pub fn failures(&self) -> &HashMap<PartAlias, ResolutionError> {
        &self.failures
    }

    /// Returns `true` if any sub-part reference failed to resolve.
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

/// Splits a resolver state map into successfully-resolved entries and failed
/// entries (those that ended in [`ResolutionState::Missing`]). Other states
/// (`Pending`, `Subpart`) are discarded.
fn partition_resolver_map(
    map: HashMap<PartAlias, ResolutionState>,
    failures: &mut HashMap<PartAlias, ResolutionError>,
) -> HashMap<PartAlias, Arc<MultipartDocument>> {
    let mut entries = HashMap::new();
    for (k, v) in map {
        match v {
            ResolutionState::Associated(e) => {
                entries.insert(k, e);
            }
            ResolutionState::Missing(err) => {
                failures.insert(k, err);
            }
            ResolutionState::Pending | ResolutionState::Subpart => {}
        }
    }
    entries
}

pub async fn resolve_dependencies_multipart<F, L>(
    document: &MultipartDocument,
    cache: Arc<RwLock<PartCache>>,
    colors: &ColorCatalog,
    loader: &L,
    on_update: &F,
) -> ResolutionResult
where
    F: Fn(PartAlias, Result<(), &ResolutionError>),
    L: LibraryLoader,
{
    let mut resolver = DependencyResolver::new(colors, cache, on_update, loader);

    resolver.scan_dependencies_with_parent(None, document, true);
    while resolver.resolve_pending_dependencies().await {}

    let mut failures = HashMap::new();
    let library_entries = partition_resolver_map(resolver.map, &mut failures);
    let local_entries = partition_resolver_map(resolver.local_map, &mut failures);

    ResolutionResult {
        library_entries,
        local_entries,
        failures,
    }
}

pub async fn resolve_dependencies<F, L>(
    document: &Document,
    cache: Arc<RwLock<PartCache>>,
    colors: &ColorCatalog,
    loader: &L,
    on_update: &F,
) -> ResolutionResult
where
    F: Fn(PartAlias, Result<(), &ResolutionError>),
    L: LibraryLoader,
{
    let mut resolver = DependencyResolver::new(colors, cache, on_update, loader);

    resolver.scan_dependencies(document, true);
    while resolver.resolve_pending_dependencies().await {}

    let mut failures = HashMap::new();
    let library_entries = partition_resolver_map(resolver.map, &mut failures);
    let local_entries = partition_resolver_map(resolver.local_map, &mut failures);

    ResolutionResult {
        library_entries,
        local_entries,
        failures,
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use super::{PartCache, PartKind};
    use crate::{
        PartAlias,
        document::{BfcCertification, Document, MultipartDocument},
    };

    #[test]
    fn test_part_cache_query_existing() {
        let document = MultipartDocument {
            body: Document {
                name: "Doc".to_string(),
                author: "Author".to_string(),
                description: "Description".to_string(),
                bfc: BfcCertification::NoCertify,
                headers: vec![],
                commands: vec![],
            },
            subparts: HashMap::new(),
        };

        let mut cache = PartCache::new();

        let existing_key = PartAlias::from("existing".to_string());
        let document = Arc::new(document);

        cache.register(PartKind::Primitive, existing_key.clone(), document.clone());

        assert_eq!(cache.query(&existing_key).unwrap(), document);
    }

    #[test]
    fn test_part_cache_query_missing() {
        let cache = PartCache::new();
        let missing_key = PartAlias::from("missing".to_string());

        assert!(cache.query(&missing_key).is_none());
    }

    use async_trait::async_trait;
    use std::sync::RwLock;

    use super::{
        FileLocation, LibraryLoader, ResolutionResult, resolve_dependencies_multipart,
    };
    use crate::{
        color::{ColorCatalog, ColorReference},
        elements::{Command, PartReference},
        error::ResolutionError,
        Matrix4,
    };

    /// A loader that always returns `FileNotFound` from `load_ref`. Used to
    /// verify that resolution failures surface to `ResolutionResult::failures`.
    struct AlwaysMissingLoader;

    #[async_trait(?Send)]
    impl LibraryLoader for AlwaysMissingLoader {
        async fn load_colors(&self) -> Result<ColorCatalog, ResolutionError> {
            Ok(ColorCatalog::new())
        }

        async fn load_ref(
            &self,
            _alias: PartAlias,
            _local: bool,
            _colors: &ColorCatalog,
        ) -> Result<(FileLocation, MultipartDocument), ResolutionError> {
            Err(ResolutionError::FileNotFound)
        }
    }

    fn doc_referencing(alias: &str) -> MultipartDocument {
        MultipartDocument {
            body: Document {
                name: "root".to_string(),
                author: "test".to_string(),
                description: "doc with one missing sub-part".to_string(),
                bfc: BfcCertification::NoCertify,
                headers: vec![],
                commands: vec![Command::PartReference(PartReference {
                    color: ColorReference::Current,
                    matrix: Matrix4::from_cols(
                        [1.0, 0.0, 0.0, 0.0].into(),
                        [0.0, 1.0, 0.0, 0.0].into(),
                        [0.0, 0.0, 1.0, 0.0].into(),
                        [0.0, 0.0, 0.0, 1.0].into(),
                    ),
                    name: PartAlias::from(alias.to_string()),
                })],
            },
            subparts: HashMap::new(),
        }
    }

    #[test]
    fn resolution_result_surfaces_missing_subpart_with_noop_callback() {
        let doc = doc_referencing("does-not-exist.dat");
        let cache = Arc::new(RwLock::new(PartCache::default()));
        let colors = ColorCatalog::new();
        let loader = AlwaysMissingLoader;

        let result: ResolutionResult = futures::executor::block_on(
            resolve_dependencies_multipart(&doc, cache, &colors, &loader, &|_, _| {}),
        );

        assert!(
            result.has_failures(),
            "missing sub-part must surface even with a no-op callback"
        );
        assert!(
            result
                .failures()
                .contains_key(&PartAlias::from("does-not-exist.dat".to_string())),
            "the specific missing alias should appear in failures(): {:?}",
            result.failures().keys().collect::<Vec<_>>()
        );
        assert!(
            matches!(
                result.failures().values().next().unwrap(),
                ResolutionError::FileNotFound
            ),
            "the error from load_ref should be preserved verbatim"
        );
    }
}
