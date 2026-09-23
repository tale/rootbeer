use crate::LockedPackage;
use serde::{Deserialize, Serialize};

/// Immutable artifact facts and the digest of its accompanying build receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedArtifact {
    pub revision: u32,
    pub receipt_sha256: String,
    pub package: LockedPackage,
}

/// The test catalog and a published `xz` artifact for aarch64-linux.
#[cfg(test)]
pub(crate) fn fixture() -> (crate::PackageCatalog, PublishedArtifact) {
    use crate::{ArchiveFormat, LockedInstall, LockedSource, Provides};
    let catalog = crate::test_catalog::catalog().clone();
    let entry = &catalog.packages["xz"];
    let version = entry.default_version_for("aarch64-linux").unwrap();
    let published = &entry.versions[version];
    let recipe = published.for_system("aarch64-linux").unwrap();
    let package = LockedPackage {
        name: entry.name.clone(),
        version: version.to_string(),
        source: LockedSource::Url {
            url: "https://packages.example/xz.tar.gz".into(),
            sha256: "a".repeat(64),
        },
        install: LockedInstall::Archive {
            format: ArchiveFormat::TarGz,
            strip_prefix: None,
        },
        provides: Provides {
            bins: recipe.bins.paths().cloned().unwrap_or_default(),
            apps: Default::default(),
        },
        runtime_dependencies: Default::default(),
        output_sha256: Some("b".repeat(64)),
    };
    let artifact = PublishedArtifact {
        revision: published.revision,
        receipt_sha256: "c".repeat(64),
        package,
    };
    (catalog, artifact)
}
