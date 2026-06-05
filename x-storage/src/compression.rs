// Stub module - compression removed in x.engine.
// All writes use uncompressed JSON.

use std::io;

pub const COMPRESSION_THRESHOLD_BYTES: usize = 4096;

/// Uncompressed passthrough (compression disabled).
/// Returns serialized JSON bytes.
pub fn compress(value: &serde_json::Value) -> io::Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

/// Uncompressed passthrough for raw bytes.
pub fn compress_bytes(data: &[u8]) -> io::Result<Vec<u8>> {
    Ok(data.to_vec())
}

/// Uncompressed passthrough - deserializes bytes back to JSON Value.
pub fn decompress(data: &[u8]) -> io::Result<serde_json::Value> {
    serde_json::from_slice(data).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}
