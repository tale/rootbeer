//! What a PDR publishes and a client verifies.
//!
//! This crate owns the wire format: the published types, canonical JSON, and signature
//! verification. It depends on nothing from the store, build or resolution layers, so
//! internal types cannot leak into the contract.

mod pin;

pub use pin::{is_sha256, validate_https, PackageIndexPin};
