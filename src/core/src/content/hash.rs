use sha2::{Digest, Sha256};

/// Compute full SHA-256 hex digest (64 chars). Used for ContentHash column.
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    const_hex::encode(hasher.finalize())
}

/// Compute first 8 bytes of SHA-256 as a u64 (little-endian).
/// Used for fast in-memory paste suppression HashSet — not collision-resistant.
pub fn sha256_u64(content: &str) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let bytes = hasher.finalize();
    let arr: [u8; 8] = bytes[..8].try_into().expect("SHA-256 output is >= 8 bytes");
    u64::from_le_bytes(arr)
}

/// Normalize line endings before hashing to ensure consistent deduplication.
/// Replaces \r\n → \n.
pub fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n")
}
