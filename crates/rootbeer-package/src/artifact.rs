use crate::{LockedPackage, PackageCatalog};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A catalog snapshot and the exact artifacts available for each package and system.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIndex {
    pub schema: u32,
    pub catalog: PackageCatalog,
    pub catalog_sha256: String,
    pub artifacts: BTreeMap<String, BTreeMap<String, PublishedArtifact>>,
}

/// Immutable artifact facts and the digest of its accompanying build receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedArtifact {
    pub revision: u32,
    pub receipt_sha256: String,
    pub package: LockedPackage,
}

#[cfg(test)]
pub(crate) fn fixture() -> ArtifactIndex {
    use crate::{ArchiveFormat, LockedInstall, LockedSource, Provides};
    let catalog = PackageCatalog::embedded().unwrap().clone();
    let entry = &catalog.packages["xz"];
    let recipe = &entry.versions[&entry.default_version];
    let package = LockedPackage {
        name: entry.name.clone(),
        version: entry.default_version.clone(),
        source: LockedSource::Url {
            url: "https://packages.example/xz.tar.gz".into(),
            sha256: "a".repeat(64),
        },
        install: LockedInstall::Archive {
            format: ArchiveFormat::TarGz,
            strip_prefix: None,
        },
        provides: Provides {
            bins: recipe
                .bins
                .iter()
                .map(|bin| (bin.clone(), std::path::PathBuf::from("bin").join(bin)))
                .collect(),
            apps: Default::default(),
        },
        runtime_dependencies: Default::default(),
        output_sha256: Some("b".repeat(64)),
    };
    ArtifactIndex {
        schema: ArtifactIndex::schema_for(&catalog),
        catalog_sha256: catalog.sha256(),
        artifacts: BTreeMap::from([(
            package.id(),
            BTreeMap::from([(
                "aarch64-linux".into(),
                PublishedArtifact {
                    revision: recipe.revision,
                    receipt_sha256: "c".repeat(64),
                    package,
                },
            )]),
        )]),
        catalog,
    }
}
