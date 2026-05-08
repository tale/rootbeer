use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub source: LockedSource,
    pub install: LockedInstall,
    pub provides: Provides,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockedSource {
    /// A local tree with a locked deterministic tree hash. This is mostly a
    /// bootstrap/testing source until URL/archive fetching is implemented.
    Path { path: PathBuf, sha256: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockedInstall {
    /// Install a directory tree from the source. `strip_prefix`, when present,
    /// selects a relative subdirectory from the source as the output root.
    Directory { strip_prefix: Option<PathBuf> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provides {
    /// Binary name to path inside the installed output tree.
    pub bins: BTreeMap<String, PathBuf>,
}
