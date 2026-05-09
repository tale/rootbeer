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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_sha256: Option<String>,
}

impl LockedPackage {
    pub fn id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }

    pub fn same_locked_facts_without_output_hash(&self, other: &Self) -> bool {
        self.name == other.name
            && self.version == other.version
            && self.source == other.source
            && self.install == other.install
            && self.provides == other.provides
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockedSource {
    /// A local tree with a locked deterministic tree hash.
    Path { path: PathBuf, sha256: String },

    /// A local source file with a locked byte hash.
    File { path: PathBuf, sha256: String },

    /// A remote source file with a locked byte hash. Apply never interprets
    /// mutable remote metadata; the URL and hash are already locked facts.
    Url { url: String, sha256: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockedInstall {
    /// Install a directory tree from the source. `strip_prefix`, when present,
    /// selects a relative subdirectory from the source as the output root.
    Directory { strip_prefix: Option<PathBuf> },

    /// Install an archive source. `strip_prefix`, when present, selects a
    /// relative subdirectory from the extracted archive as the output root.
    Archive {
        format: ArchiveFormat,
        strip_prefix: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchiveFormat {
    TarGz,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provides {
    /// Binary name to path inside the installed output tree.
    pub bins: BTreeMap<String, PathBuf>,
}
