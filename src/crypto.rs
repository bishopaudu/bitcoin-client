// src/crypto.rs — Cryptographic utility functions

use sha2::{Sha256, Digest};

// Computes the double SHA-256 hash of the input data.
pub fn double_sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let first = hasher.finalize();

    let mut hasher = Sha256::new();
    hasher.update(&first);
    hasher.finalize().into()
}

// Encodes a byte slice into a lowercase hexadecimal string.
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}