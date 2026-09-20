use rootbeer_package::{ArtifactIndex, PublishedArtifact};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    ArchiveFormat, BuildArtifact, LockedInstall, LockedSource, PackageCatalog, PackageRealizer,
};
use rootbeer_package::index::is_sha256;
use rootbeer_store::{hash_bytes, hash_file, Store};

/// Verifies build outputs and assembles an index, archives, and receipts for hosting.
/// No artifact commands run, and no files are uploaded. The output must not exist.
pub fn bundle_artifacts(
    catalog: &PackageCatalog,
    receipts: &[PathBuf],
    base_url: &str,
    output: &Path,
) -> Result<String, String> {
    catalog.validate()?;
    validate_base_url(base_url)?;
    if receipts.is_empty() {
        return Err("bundle requires at least one build receipt".into());
    }
    let staging = super::publication::staging(output)?;
    let destination = staging.path().join("bundle");
    super::publication::create_bundle(&destination)?;
    let realizer = PackageRealizer::with_dirs(
        Store::new(staging.path().join("store")),
        staging.path().join("downloads"),
        staging.path().join("install"),
    );
    let mut index = ArtifactIndex {
        schema: ArtifactIndex::schema_for(catalog),
        catalog: catalog.clone(),
        catalog_sha256: catalog.sha256(),
        artifacts: BTreeMap::new(),
    };
    for receipt_path in receipts {
        let (system, artifact, bytes) =
            prepare_artifact(catalog, receipt_path, base_url, &destination, &realizer)?;
        let key = artifact.package.id();
        fs::write(
            destination
                .join("receipts")
                .join(format!("{}.json", artifact.receipt_sha256)),
            bytes,
        )
        .map_err(|e| e.to_string())?;
        if index
            .artifacts
            .entry(key.clone())
            .or_default()
            .insert(system.clone(), artifact)
            .is_some()
        {
            return Err(format!("duplicate artifact for {key} on {system}"));
        }
    }

    index.validate()?;
    let bytes = serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?;
    let digest = hash_bytes(&bytes);
    fs::write(destination.join("index.json"), bytes).map_err(|e| e.to_string())?;
    fs::write(
        destination.join("index.sha256"),
        format!("{digest}  index.json\n"),
    )
    .map_err(|e| e.to_string())?;
    fs::rename(destination, output).map_err(|e| e.to_string())?;
    Ok(digest)
}

pub(super) fn prepare_artifact(
    catalog: &PackageCatalog,
    receipt_path: &Path,
    base_url: &str,
    destination: &Path,
    realizer: &PackageRealizer,
) -> Result<(String, PublishedArtifact, Vec<u8>), String> {
    validate_base_url(base_url)?;
    let bytes = fs::read(receipt_path).map_err(|e| format!("{}: {e}", receipt_path.display()))?;
    let receipt: BuildArtifact = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    validate_receipt(catalog, &receipt)?;
    let key = receipt.package.id();
    let LockedSource::File { sha256, .. } = &receipt.package.source else {
        return Err(format!("{key}: expected a source-build file artifact"));
    };
    let sha256 = sha256.clone();
    // Receipts move between CI runners; never follow the builder's absolute paths.
    let source = receipt_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("package.tar.gz");
    let target = destination
        .join("artifacts")
        .join(format!("{sha256}.tar.gz"));
    fs::copy(source, &target).map_err(|e| e.to_string())?;
    if hash_file(&target).map_err(|e| e.to_string())? != sha256 {
        return Err(format!("{key}: artifact hash mismatch"));
    }
    let mut package = receipt.package;
    copy_runtime(
        &mut package,
        receipt_path.parent().unwrap_or(Path::new(".")),
        destination,
    )?;
    package.source = LockedSource::File {
        path: target,
        sha256: sha256.clone(),
    };
    let realized = realizer
        .realize(&package)
        .map_err(|e| format!("{key}: {e}"))?;
    let mut runtime = BTreeMap::new();
    for dependency in rootbeer_package::runtime::closure(&package)? {
        let realized = realizer.realize(dependency).map_err(|e| e.to_string())?;
        runtime.insert(
            rootbeer_package::runtime::store_directory(dependency)?,
            realized.store_entry.path,
        );
    }
    let report = rootbeer_build::audit::audit_with_runtime(&realized.store_entry.path, &runtime)?;
    report.validate()?;
    if let Some(expected) = &receipt.runtime_audit_sha256 {
        let report = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
        if hash_bytes(&report) != *expected {
            return Err(format!("{key}: runtime audit digest mismatch"));
        }
    }
    package.source = LockedSource::Url {
        url: if base_url.starts_with("ghcr://") {
            format!("{base_url}@sha256:{sha256}")
        } else {
            format!(
                "{}/artifacts/{sha256}.tar.gz",
                base_url.trim_end_matches('/')
            )
        },
        sha256: sha256.clone(),
    };
    publish_runtime(&mut package, base_url)?;
    let receipt_sha256 = hash_bytes(&bytes);
    Ok((
        receipt.system,
        PublishedArtifact {
            revision: receipt.revision,
            receipt_sha256,
            package,
        },
        bytes,
    ))
}

fn copy_runtime(
    package: &mut rootbeer_package::LockedPackage,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    for dependency in package.runtime_dependencies.values_mut() {
        copy_runtime(dependency, source, destination)?;
        let LockedSource::File { sha256, .. } = &dependency.source else {
            return Err("runtime receipts require local archives".into());
        };
        let archive = source.join("runtime").join(format!(
            "{}.tar.gz",
            rootbeer_package::runtime::store_directory(dependency)?.display()
        ));
        let target = destination
            .join("artifacts")
            .join(format!("{sha256}.tar.gz"));
        fs::copy(archive, &target).map_err(|e| e.to_string())?;
        if hash_file(&target).map_err(|e| e.to_string())? != *sha256 {
            return Err(format!(
                "{}: runtime archive hash mismatch",
                dependency.id()
            ));
        }
        dependency.source = LockedSource::File {
            path: target,
            sha256: sha256.clone(),
        };
    }
    Ok(())
}

fn publish_runtime(
    package: &mut rootbeer_package::LockedPackage,
    base_url: &str,
) -> Result<(), String> {
    for dependency in package.runtime_dependencies.values_mut() {
        publish_runtime(dependency, base_url)?;
        let LockedSource::File { sha256, .. } = &dependency.source else {
            return Err("runtime bundle requires local archives".into());
        };
        dependency.source = LockedSource::Url {
            url: if base_url.starts_with("ghcr://") {
                format!("{base_url}@sha256:{sha256}")
            } else {
                format!(
                    "{}/artifacts/{sha256}.tar.gz",
                    base_url.trim_end_matches('/')
                )
            },
            sha256: sha256.clone(),
        };
    }
    Ok(())
}

fn validate_base_url(url: &str) -> Result<(), String> {
    if let Some(repository) = url.strip_prefix("ghcr://") {
        return rootbeer_package::ghcr::validate_repository(repository);
    }
    let uri: ureq::http::Uri = url
        .parse()
        .map_err(|e| format!("invalid bundle base URL: {e}"))?;
    if uri.scheme_str() != Some("https")
        || uri.host().is_none_or(str::is_empty)
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#' | '@' | '\\'))
    {
        return Err(
            "bundle base URL needs HTTPS and a host, without credentials, query, or fragment"
                .into(),
        );
    }
    Ok(())
}

fn validate_receipt(catalog: &PackageCatalog, receipt: &BuildArtifact) -> Result<(), String> {
    let package = &receipt.package;
    let recipe = catalog
        .packages
        .get(&package.name)
        .and_then(|entry| entry.versions.get(&package.version))
        .ok_or_else(|| format!("{}: no matching catalog recipe", package.id()))?;
    let Some(build) = &recipe.build else {
        return Err(format!("{}: not a source recipe", package.id()));
    };
    if !matches!(receipt.schema, 1 | 2)
        || (receipt.schema < 2 && !package.runtime_dependencies.is_empty())
        || receipt.revision != recipe.revision
        || receipt.recipe_sha256 != recipe.sha256()
        || !recipe.systems.contains(&receipt.system)
        || serde_json::to_value(&receipt.build).map_err(|e| e.to_string())?
            != serde_json::to_value(build).map_err(|e| e.to_string())?
    {
        return Err(format!(
            "{}: receipt does not match catalog inputs",
            package.id()
        ));
    }
    if package.install
        != (LockedInstall::Archive {
            format: ArchiveFormat::TarGz,
            strip_prefix: None,
        })
        || package
            .output_sha256
            .as_deref()
            .is_none_or(|sha| !is_sha256(sha))
        || !matches!(&package.source, LockedSource::File { sha256, .. } if is_sha256(sha256))
        || package.provides.apps != recipe.apps
        || package.provides.bins.len() != recipe.bins.len()
        || recipe
            .bins
            .iter()
            .any(|bin| package.provides.bins.get(bin) != Some(&PathBuf::from("bin").join(bin)))
    {
        return Err(format!(
            "{}: invalid artifact hash, layout, or commands",
            package.id()
        ));
    }
    let graph =
        rootbeer_package::graph::DependencyGraph::new(catalog, &[package.id()], &receipt.system)?;
    let closure = &graph.nodes[&package.id()].closure;
    if receipt.dependencies.len() != closure.len()
        || closure.iter().any(|dependency| {
            receipt
                .dependencies
                .get(dependency)
                .is_none_or(|package| package.id() != *dependency)
        })
    {
        return Err(format!(
            "{}: build dependency receipts do not match",
            package.id()
        ));
    }
    let runtime = rootbeer_package::runtime::closure(package)?;
    if runtime
        .iter()
        .map(|package| package.id())
        .collect::<std::collections::BTreeSet<_>>()
        != graph.nodes[&package.id()]
            .runtime_closure
            .iter()
            .cloned()
            .collect()
    {
        return Err("receipt runtime closure does not match dependency roles".into());
    }
    for runtime_package in runtime.into_iter().chain(std::iter::once(package)) {
        let direct: std::collections::BTreeSet<_> = graph.nodes[&runtime_package.id()]
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind().is_runtime())
            .map(|dependency| dependency.package())
            .collect();
        if direct
            != runtime_package
                .runtime_dependencies
                .keys()
                .map(String::as_str)
                .collect()
        {
            return Err("receipt runtime edges do not match dependency roles".into());
        }
        if runtime_package.id() == package.id() {
            continue;
        }
        if !matches!(&runtime_package.source, LockedSource::File { sha256, .. } if is_sha256(sha256))
            || runtime_package.install
                != (LockedInstall::Archive {
                    format: ArchiveFormat::TarGz,
                    strip_prefix: None,
                })
        {
            return Err("runtime receipts require hashed tar.gz archives".into());
        }
        let expected = &receipt.dependencies[&runtime_package.id()];
        if runtime_package.output_sha256 != expected.output_sha256
            || runtime_package.provides != expected.provides
        {
            return Err("receipt runtime output differs from build dependency".into());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{LockedPackage, PackageResolverInputs, Provides};
    use flate2::{write::GzEncoder, Compression};
    use rootbeer_store::hash_tree;
    use std::os::unix::fs::PermissionsExt;

    pub(crate) fn fixture(root: &Path) -> (PackageCatalog, PathBuf) {
        let catalog = crate::test_catalog::catalog().clone();
        let entry = &catalog.packages["xz"];
        let recipe = &entry.versions[&entry.default_version];
        let tree = root.join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        for bin in &recipe.bins {
            let path = tree.join("bin").join(bin);
            fs::write(&path, b"bundle test executable\n").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let build = root.join("build");
        fs::create_dir(&build).unwrap();
        let archive = build.join("package.tar.gz");
        let mut writer = tar::Builder::new(GzEncoder::new(
            fs::File::create(&archive).unwrap(),
            Compression::default(),
        ));
        writer.append_dir_all(".", &tree).unwrap();
        writer.into_inner().unwrap().finish().unwrap();
        let receipt = BuildArtifact {
            schema: 1,
            recipe_sha256: recipe.sha256(),
            build_key: None,
            build_environment: None,
            environment: None,
            isolation: None,
            runtime_audit_sha256: None,
            catalog_sha256: catalog.sha256(),
            revision: recipe.revision,
            system: "aarch64-linux".into(),
            build: recipe.build.clone().unwrap(),
            dependencies: BTreeMap::new(),
            resolver_inputs: PackageResolverInputs::default(),
            toolchain: BTreeMap::new(),
            package: LockedPackage {
                name: entry.name.clone(),
                version: entry.default_version.clone(),
                source: LockedSource::File {
                    path: PathBuf::from("/unavailable/runner/package.tar.gz"),
                    sha256: hash_file(&archive).unwrap(),
                },
                install: LockedInstall::Archive {
                    format: ArchiveFormat::TarGz,
                    strip_prefix: None,
                },
                provides: Provides {
                    apps: Default::default(),
                    bins: recipe
                        .bins
                        .iter()
                        .map(|bin| (bin.clone(), PathBuf::from("bin").join(bin)))
                        .collect(),
                },
                runtime_dependencies: Default::default(),
                output_sha256: Some(hash_tree(&tree).unwrap()),
            },
        };
        let path = build.join("receipt.json");
        fs::write(&path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
        (catalog, path)
    }

    #[test]
    fn bundles_relocated_receipts_deterministically_without_executing_commands() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = fixture(root.path());
        let mut other: BuildArtifact =
            serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        other.system = "x86_64-linux".into();
        let other_path = receipt.with_file_name("other-receipt.json");
        fs::write(&other_path, serde_json::to_vec(&other).unwrap()).unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        let digest = bundle_artifacts(
            &catalog,
            &[receipt.clone(), other_path.clone()],
            "https://packages.example/rootbeer/",
            &first,
        )
        .unwrap();
        assert_eq!(
            digest,
            bundle_artifacts(
                &catalog,
                &[other_path, receipt],
                "https://packages.example/rootbeer",
                &second
            )
            .unwrap()
        );
        let bytes = fs::read(first.join("index.json")).unwrap();
        assert_eq!(hash_bytes(&bytes), digest);
        assert_eq!(bytes, fs::read(second.join("index.json")).unwrap());
        let index: ArtifactIndex = serde_json::from_slice(&bytes).unwrap();
        let systems = index.artifacts.values().next().unwrap();
        assert_eq!(systems.len(), 2);
        let artifact = &systems["aarch64-linux"];
        let LockedSource::Url { url, sha256 } = &artifact.package.source else {
            panic!("expected portable URL")
        };
        assert_eq!(
            url,
            &format!("https://packages.example/rootbeer/artifacts/{sha256}.tar.gz")
        );
        assert_eq!(
            hash_file(first.join("artifacts").join(format!("{sha256}.tar.gz"))).unwrap(),
            *sha256
        );
        assert!(first
            .join("receipts")
            .join(format!("{}.json", artifact.receipt_sha256))
            .is_file());
    }

    #[test]
    fn bundles_ghcr_archive_digests_without_container_manifests() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = fixture(root.path());
        let output = root.path().join("ghcr");
        bundle_artifacts(&catalog, &[receipt], "ghcr://tale/rootbeer/xz", &output).unwrap();
        let mut index: ArtifactIndex =
            serde_json::from_slice(&fs::read(output.join("index.json")).unwrap()).unwrap();
        let package = &mut index
            .artifacts
            .values_mut()
            .next()
            .unwrap()
            .values_mut()
            .next()
            .unwrap()
            .package;
        let LockedSource::Url { url, sha256 } = &mut package.source else {
            panic!("expected GHCR source")
        };
        assert_eq!(url, &format!("ghcr://tale/rootbeer/xz@sha256:{sha256}"));
        *url = format!("ghcr://tale/rootbeer/xz@sha256:{}", "0".repeat(64));
        assert!(index.validate().unwrap_err().contains("does not match"));
    }

    #[test]
    fn bundles_runtime_archives_and_rejects_missing_or_tampered_inputs() {
        let root = tempfile::tempdir().unwrap();
        let (mut catalog, path) = fixture(root.path());
        let mut receipt: BuildArtifact = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let version = receipt.package.version.clone();
        let mut definition = catalog.packages["xz"].clone();
        definition.name = "runtime-base".into();
        definition.aliases.clear();
        catalog.packages.insert(definition.name.clone(), definition);
        let dependency_id = format!("runtime-base@{version}");
        let recipe = catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .get_mut(&version)
            .unwrap();
        recipe.build.as_mut().unwrap().dependencies =
            vec![rootbeer_package::BuildDependency::Scoped {
                package: dependency_id.clone(),
                kind: rootbeer_package::DependencyKind::Runtime,
            }];
        receipt.build = recipe.build.clone().unwrap();
        receipt.recipe_sha256 = recipe.sha256();
        receipt.schema = 2;
        receipt.catalog_sha256 = catalog.sha256();
        let tree = root.path().join("tree");
        fs::write(tree.join("runtime-data"), b"runtime fixture").unwrap();
        let mut dependency = receipt.package.clone();
        dependency.name = "runtime-base".into();
        dependency.output_sha256 = Some(hash_tree(&tree).unwrap());
        let runtime = path.parent().unwrap().join("runtime");
        fs::create_dir(&runtime).unwrap();
        let archive = runtime.join(format!(
            "{}.tar.gz",
            rootbeer_package::runtime::store_directory(&dependency)
                .unwrap()
                .display()
        ));
        rootbeer_build::pack(&tree, &archive).unwrap();
        let original = fs::read(&archive).unwrap();
        dependency.source = LockedSource::File {
            path: "/unavailable/runtime.tar.gz".into(),
            sha256: hash_file(&archive).unwrap(),
        };
        receipt
            .dependencies
            .insert(dependency_id.clone(), dependency.clone());
        receipt
            .package
            .runtime_dependencies
            .insert(dependency_id.clone(), dependency);
        fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        let output = root.path().join("bundle");
        fs::write(&archive, b"tampered").unwrap();
        assert!(bundle_artifacts(
            &catalog,
            std::slice::from_ref(&path),
            "ghcr://owner/index/xz",
            &output
        )
        .unwrap_err()
        .contains("runtime archive hash mismatch"));
        assert!(!output.exists());
        fs::remove_file(&archive).unwrap();
        assert!(bundle_artifacts(
            &catalog,
            std::slice::from_ref(&path),
            "ghcr://owner/index/xz",
            &output
        )
        .is_err());
        fs::write(&archive, original).unwrap();
        bundle_artifacts(&catalog, &[path], "ghcr://owner/index/xz", &output).unwrap();
        let index: ArtifactIndex =
            serde_json::from_slice(&fs::read(output.join("index.json")).unwrap()).unwrap();
        assert_eq!(index.schema, 6);
        let package = &index.artifacts[&receipt.package.id()][&receipt.system].package;
        let dependency = &package.runtime_dependencies[&dependency_id];
        let LockedSource::Url { url, sha256 } = &dependency.source else {
            panic!("expected published runtime archive")
        };
        assert_eq!(url, &format!("ghcr://owner/index/xz@sha256:{sha256}"));
        assert_eq!(
            hash_file(output.join("artifacts").join(format!("{sha256}.tar.gz"))).unwrap(),
            *sha256
        );
        let mut downgraded = index;
        downgraded.schema = 5;
        assert!(downgraded
            .validate_fragment()
            .unwrap_err()
            .contains("schema 6"));
    }

    #[test]
    fn verifies_runtime_audit_receipt_digest() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, path) = fixture(root.path());
        let mut receipt: BuildArtifact = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        receipt.runtime_audit_sha256 = Some("0".repeat(64));
        fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        let output = root.path().join("bundle");
        assert!(bundle_artifacts(
            &catalog,
            std::slice::from_ref(&path),
            "https://packages.example",
            &output
        )
        .unwrap_err()
        .contains("runtime audit digest mismatch"));
        assert!(!output.exists());

        let report = rootbeer_build::audit::audit(&root.path().join("tree")).unwrap();
        receipt.runtime_audit_sha256 =
            Some(hash_bytes(&serde_json::to_vec_pretty(&report).unwrap()));
        fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        bundle_artifacts(&catalog, &[path], "https://packages.example", &output).unwrap();
    }

    #[test]
    fn rejects_tampered_archives_without_leaving_a_bundle() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = fixture(root.path());
        fs::write(receipt.with_file_name("package.tar.gz"), b"tampered").unwrap();
        let output = root.path().join("bundle");
        assert!(
            bundle_artifacts(&catalog, &[receipt], "https://packages.example", &output)
                .unwrap_err()
                .contains("hash mismatch")
        );
        assert!(!output.exists());
    }

    #[test]
    fn rejects_invalid_receipts_and_duplicate_platforms() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, path) = fixture(root.path());
        let mut receipt: BuildArtifact = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for field in [
            "revision",
            "recipe_sha256",
            "system",
            "build",
            "output_sha256",
            "bins",
            "dependencies",
        ] {
            let mut value = serde_json::to_value(&receipt).unwrap();
            match field {
                "revision" => value[field] = serde_json::json!(99),
                "recipe_sha256" => value[field] = serde_json::json!("0".repeat(64)),
                "system" => value[field] = serde_json::json!("x86_64-windows"),
                "build" => value[field]["configure"] = serde_json::json!(["--different"]),
                "output_sha256" => value["package"][field] = serde_json::Value::Null,
                "bins" => {
                    value["package"]["provides"][field]["xz"] = serde_json::json!("../escape")
                }
                "dependencies" => {
                    value[field]["extra@1"] = serde_json::to_value(&receipt.package).unwrap()
                }
                _ => unreachable!(),
            }
            let changed = serde_json::from_value(value).unwrap();
            assert!(validate_receipt(&catalog, &changed).is_err(), "{field}");
        }
        let output = root.path().join("bundle");
        assert!(bundle_artifacts(
            &catalog,
            &[path.clone(), path.clone()],
            "https://packages.example",
            &output
        )
        .unwrap_err()
        .contains("duplicate"));
        assert!(!output.exists());
        receipt.package.output_sha256 = Some("0".repeat(64));
        fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(
            bundle_artifacts(&catalog, &[path], "https://packages.example", &output)
                .unwrap_err()
                .contains("output hash mismatch")
        );
        assert!(!output.exists());
    }

    #[test]
    fn rejects_unsafe_destinations_and_preserves_existing_output() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = fixture(root.path());
        for url in [
            "http://example.org",
            "https:///path",
            "https://user@example.org",
            "https://example.org?q=x",
            "https://example.org/#fragment",
            "https://example.org/\n",
        ] {
            assert!(bundle_artifacts(
                &catalog,
                std::slice::from_ref(&receipt),
                url,
                &root.path().join("bundle")
            )
            .is_err());
        }
        let output = root.path().join("existing");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep"), "keep").unwrap();
        assert!(
            bundle_artifacts(&catalog, &[receipt], "https://packages.example", &output)
                .unwrap_err()
                .contains("already exists")
        );
        assert_eq!(fs::read_to_string(output.join("keep")).unwrap(), "keep");
    }
}
