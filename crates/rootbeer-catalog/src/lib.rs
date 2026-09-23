//! What a PDR publishes and a client verifies.
//!
//! This crate owns the wire format: the published types, canonical JSON, and signature
//! verification. It depends on nothing from the store, build or resolution layers, so
//! internal types cannot leak into the contract.

mod canonical;
mod hex;
mod pin;

pub use canonical::{canonical_json, canonical_sha256};
pub use hex::decode_hex;
pub use pin::{is_sha256, validate_https, PackageIndexPin};
