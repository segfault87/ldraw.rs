//! Convenience wrapper for loading and baking parts by ID.
//!
//! [`PartLoader`] reduces the standard "load one part" recipe from ~7 lines
//! of ceremony (manual `.dat` suffix, `Arc<RwLock<PartCache>>`, per-call
//! `load_colors`, no-op `on_update` callback) down to a single call.
//!
//! ```no_run
//! # #[cfg(not(target_arch = "wasm32"))]
//! # async fn example() -> Result<(), ldraw::error::ResolutionError> {
//! use ldraw_ir::loader::PartLoader;
//! use std::path::PathBuf;
//!
//! let loader = PartLoader::from_library_root(PathBuf::from("/path/to/ldraw")).await?;
//! let part = loader.load_part("3001").await?;
//! # Ok(())
//! # }
//! ```

use std::sync::{Arc, RwLock};

use ldraw::{
    color::ColorCatalog,
    error::ResolutionError,
    library::{CacheCollectionStrategy, LibraryLoader, PartCache, resolve_dependencies_multipart},
    PartAlias,
};

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use ldraw::resolvers::local::LocalLoader;

use crate::part::{bake_part_from_multipart_document, Part};

/// A pre-warmed loader that caches colors and a part cache so individual
/// part loads don't have to.
///
/// Wraps any [`LibraryLoader`] together with its already-loaded
/// [`ColorCatalog`] and an internal [`PartCache`]. Use [`load_part`] to
/// go from a part ID to a baked [`Part`] in a single call.
///
/// After each successful `load_part`, the cache's part entries are
/// collected (primitives are retained), so a long-lived `PartLoader`
/// loading many distinct parts does not grow unboundedly. For full
/// control, see [`clear_cache`].
///
/// [`load_part`]: PartLoader::load_part
/// [`clear_cache`]: PartLoader::clear_cache
pub struct PartLoader<L: LibraryLoader> {
    loader: L,
    colors: ColorCatalog,
    cache: Arc<RwLock<PartCache>>,
}

impl<L: LibraryLoader> PartLoader<L> {
    /// Constructs a `PartLoader` from an existing loader and color catalog.
    ///
    /// Use this when you already have a `ColorCatalog` (e.g. from a previous
    /// call to `loader.load_colors()`) and want to avoid re-reading
    /// `LDConfig.ldr`.
    pub fn new(loader: L, colors: ColorCatalog) -> Self {
        Self {
            loader,
            colors,
            cache: Arc::new(RwLock::new(PartCache::default())),
        }
    }

    /// Constructs a `PartLoader` by calling `load_colors()` on the
    /// underlying loader once, up front.
    pub async fn with_loaded_colors(loader: L) -> Result<Self, ResolutionError> {
        let colors = loader.load_colors().await?;
        Ok(Self::new(loader, colors))
    }

    /// Returns the cached [`ColorCatalog`].
    pub fn colors(&self) -> &ColorCatalog {
        &self.colors
    }

    /// Returns a reference to the underlying loader.
    pub fn inner(&self) -> &L {
        &self.loader
    }

    /// Collects unreferenced entries from the internal [`PartCache`]
    /// according to `strategy`.
    ///
    /// `load_part` already calls this internally with
    /// [`CacheCollectionStrategy::Parts`] after each load to bound
    /// part-document growth; this method is exposed for callers that want
    /// stronger guarantees (e.g. dropping primitives too via
    /// [`CacheCollectionStrategy::PartsAndPrimitives`]).
    ///
    /// Returns the number of entries collected.
    pub fn clear_cache(&self, strategy: CacheCollectionStrategy) -> usize {
        self.cache.write().unwrap().collect(strategy)
    }

    /// Loads and bakes the part identified by `part_id`.
    ///
    /// `part_id` may be supplied with or without an LDraw file extension
    /// (`"3001"` and `"3001.dat"` are equivalent; `"foo.ldr"` and `"foo.mpd"`
    /// are passed through). Primitive documents (studs, cylinders, etc.) are
    /// retained in the internal cache across calls, so loading many parts
    /// amortizes primitive parsing. Full part documents are released after
    /// the bake to bound memory growth — see [`clear_cache`] for explicit
    /// control.
    ///
    /// [`clear_cache`]: PartLoader::clear_cache
    pub async fn load_part(&self, part_id: &str) -> Result<Part, ResolutionError> {
        let alias = alias_with_default_suffix(part_id);
        let (_, document) = self.loader.load_ref(alias, false, &self.colors).await?;
        let resolutions = resolve_dependencies_multipart(
            &document,
            Arc::clone(&self.cache),
            &self.colors,
            &self.loader,
            &|_, _| {},
        )
        .await;
        let part = bake_part_from_multipart_document(&document, &resolutions, false);
        // Drop `document` and `resolutions` so their Arc references release
        // before we collect. `Parts` (not `PartsAndPrimitives`) keeps the
        // primitives that successive loads will reuse.
        drop(resolutions);
        drop(document);
        self.cache
            .write()
            .unwrap()
            .collect(CacheCollectionStrategy::Parts);
        Ok(part)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PartLoader<LocalLoader> {
    /// Convenience constructor: build a `PartLoader` backed by a
    /// filesystem-rooted LDraw library.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// PartLoader::with_loaded_colors(LocalLoader::new(Some(root), None)).await
    /// ```
    ///
    /// Unavailable on `wasm32` targets (the underlying [`LocalLoader`] is
    /// gated to non-wasm targets). On wasm, construct a `PartLoader` from
    /// an `HttpLoader` instead, via [`PartLoader::with_loaded_colors`].
    pub async fn from_library_root(root: PathBuf) -> Result<Self, ResolutionError> {
        Self::with_loaded_colors(LocalLoader::new(Some(root), None)).await
    }
}

/// If `part_id` has no LDraw extension (`.dat`, `.ldr`, `.mpd`), append
/// `.dat`. Trims surrounding whitespace.
fn alias_with_default_suffix(part_id: &str) -> PartAlias {
    let trimmed = part_id.trim();
    let lower = trimmed.to_lowercase();
    if lower.ends_with(".dat") || lower.ends_with(".ldr") || lower.ends_with(".mpd") {
        PartAlias::from(trimmed.to_string())
    } else {
        PartAlias::from(format!("{trimmed}.dat"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_with_default_suffix_appends_dat() {
        assert_eq!(
            alias_with_default_suffix("3001").original,
            "3001.dat",
        );
        assert_eq!(
            alias_with_default_suffix("  3001 ").original,
            "3001.dat",
            "whitespace should be trimmed before suffix check",
        );
    }

    #[test]
    fn alias_with_default_suffix_preserves_existing_extension() {
        for input in ["3001.dat", "model.ldr", "scene.mpd", "3001.DAT"] {
            assert_eq!(
                alias_with_default_suffix(input).original,
                input.trim(),
                "should not add .dat to {input}",
            );
        }
    }
}
