use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldEncryptor;

impl FieldEncryptor {
    pub fn new(_key: &[u8]) -> Self {
        Self
    }
    pub fn encrypt(&self, _plaintext: &str) -> String {
        String::new()
    }
    pub fn decrypt(&self, _ciphertext: &str) -> Option<String> {
        None
    }
}
