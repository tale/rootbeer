//! What a PDR publishes and a client verifies.
//!
//! This crate owns the wire format: the published types, canonical JSON, and signature
//! verification. It depends on nothing from the store, build or resolution layers, so
//! internal types cannot leak into the contract.

mod canonical;
mod hex;
mod validate;

pub use canonical::{canonical_json, canonical_sha256};
pub use hex::decode_hex;
pub use validate::{is_sha256, validate_https};
