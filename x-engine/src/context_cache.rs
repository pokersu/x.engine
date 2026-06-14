//! In-memory LRU cache for resolved artifact-offloaded context values.
//!
//! When a `$ref` marker in `context.data` is resolved from the artifact store
//! (S3/MinIO/local FS), the resolved value is cached here so that subsequent
//! accesses across steps and ticks avoid a redundant read.

use std::sync::Arc;

use moka::sync::Cache;
use serde_json::Value;

/// Default maximum number of entries in the context cache.
const DEFAULT_MAX_ENTRIES: u64 = 10_000;

/// Thread-safe LRU cache for resolved artifact offload values.
///
/// Key = artifact key (e.g. `"<instance_uuid>/ctx/<field_name>"`)
/// Value = the parsed JSON value that was stored in the artifact store.
#[derive(Clone)]
pub struct ContextCache {
    cache: Cache<String, Arc<Value>>,
}

impl ContextCache {
    /// Create a new context cache with the default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Cache::new(DEFAULT_MAX_ENTRIES),
        }
    }

    /// Create a new context cache with a specific maximum entry count.
    #[must_use]
    pub fn with_capacity(max_entries: u64) -> Self {
        Self {
            cache: Cache::new(max_entries),
        }
    }

    /// Get a cached value by artifact key. Returns `None` if absent.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Value> {
        self.cache.get(key).map(|v| (*v).clone())
    }

    /// Insert a value into the cache.
    pub fn set(&self, key: String, value: Value) {
        self.cache.insert(key, Arc::new(value));
    }

    /// Invalidate a specific cache entry.
    pub fn invalidate(&self, key: &str) {
        self.cache.invalidate(key);
    }
}

impl Default for ContextCache {
    fn default() -> Self {
        Self::new()
    }
}
