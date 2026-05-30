use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::deterministic::DeterministicInput;

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

    pub fn realization_input(&self) -> PackageRealizationInput<'_> {
        PackageRealizationInput {
            name: &self.name,
            version: &self.version,
            source: &self.source,
            install: &self.install,
            provides: &self.provides,
        }
    }

    pub fn same_realization_input(&self, other: &Self) -> bool {
        self.realization_input() == other.realization_input()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PackageRealizationInput<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub source: &'a LockedSource,
    pub install: &'a LockedInstall,
    pub provides: &'a Provides,
}

impl DeterministicInput for PackageRealizationInput<'_> {
    const KIND: &'static str = "package.realization";
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

#[cfg(test)]
mod tests {
    use super::*;

    fn package(output_sha256: Option<&str>) -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Url {
                url: "https://example.com/demo.tar.gz".to_string(),
                sha256: "source".to_string(),
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: Some(PathBuf::from("demo")),
            },
            provides: Provides {
                bins: BTreeMap::from([("demo".to_string(), PathBuf::from("bin/demo"))]),
            },
            output_sha256: output_sha256.map(str::to_string),
        }
    }

    #[test]
    fn realization_input_ignores_realized_output_hash() {
        let planned = package(None);
        let locked = package(Some("output"));

        assert_eq!(planned.realization_input(), locked.realization_input());
        assert_eq!(
            planned.realization_input().fingerprint().unwrap(),
            locked.realization_input().fingerprint().unwrap()
        );
    }

    #[test]
    fn realization_input_tracks_locked_source_facts() {
        let planned = package(None);
        let mut changed = package(None);
        changed.source = LockedSource::Url {
            url: "https://example.com/other.tar.gz".to_string(),
            sha256: "source".to_string(),
        };

        assert_ne!(planned.realization_input(), changed.realization_input());
        assert_ne!(
            planned.realization_input().fingerprint().unwrap(),
            changed.realization_input().fingerprint().unwrap()
        );
    }
}
