// Stub — encryption removed in x.engine.
// All writes are plaintext.

use std::sync::Arc;

/// EncryptingStorage wrapper — no-op passthrough in x.engine.
pub struct EncryptingStorage {
    inner: Arc<dyn crate::StorageBackend>,
}

impl EncryptingStorage {
    pub fn new(inner: Arc<dyn crate::StorageBackend>, _key: &[u8]) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<dyn crate::StorageBackend> {
        &self.inner
    }
}
