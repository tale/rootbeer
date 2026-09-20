use std::fs;
use std::path::Path;

use rootbeer_package::distribution::UpstreamProvenance;
use rootbeer_package::{
    ArchiveFormat, LockedInstall, LockedPackage, LockedSource, PackageCatalog, PackageRealizer,
    PackageRequest, PackageRequestResolver, PackageResolution, PackageResolverInputs,
    ResolveContext,
};
use rootbeer_store::{hash_file, Store};
use serde::{Deserialize, Serialize};

use crate::BuildOptions;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BinaryReceipt {
    pub schema: u32,
    pub system: String,
    pub recipe_sha256: String,
    pub package: LockedPackage,
    pub provenance: UpstreamProvenance,
}

/// Qualifies a source build or repackages a verified upstream binary without compiling it.
pub fn prepare_package(
    catalog: &PackageCatalog,
    request: &str,
    output: &Path,
    options: &BuildOptions,
) -> Result<LockedPackage, String> {
    catalog.validate()?;
    let (_, _, recipe) = rootbeer_package::graph::find_recipe(catalog, request)?;
    if recipe.build.is_some() {
        return rootbeer_build::build_package(catalog, request, output, options)
            .map(|artifact| artifact.package);
    }
    let inputs = crate::export::export_inputs(catalog, std::iter::once(request))?;
    let mut resolver = rootbeer_package::backend_stack(&inputs).with_implicit_resolver("rootbeer");
    resolver.push(rootbeer_package::catalog::CatalogResolver::new(
        catalog,
        &inputs,
        rootbeer_package::backend_stack(&inputs),
    ));
    let platform = ResolveContext::current();
    let resolution = resolver
        .resolve_package(&PackageRequest::parse(request), &platform)
        .map_err(|error| error.to_string())?;
    prepare_binary(catalog, request, output, options, resolution, inputs)
}

fn prepare_binary(
    catalog: &PackageCatalog,
    request: &str,
    output: &Path,
    options: &BuildOptions,
    resolution: PackageResolution,
    mut inputs: PackageResolverInputs,
) -> Result<LockedPackage, String> {
    let (_, _, recipe) = rootbeer_package::graph::find_recipe(catalog, request)?;
    let context = options
        .cache
        .as_ref()
        .map_or("", |cache| cache.context.as_str());
    let environment = options.environment_identity(catalog, request, context)?;
    let staging = crate::publication::staging(output)?;
    let destination = staging.path().join("result");
    fs::create_dir(&destination).map_err(|error| error.to_string())?;
    let realizer = PackageRealizer::with_dirs(
        Store::new(staging.path().join("store")),
        &options.downloads,
        staging.path().join("install"),
    )
    .with_execution(options.execution.clone());
    inputs.resolvers.remove("rootbeer");
    let proof = match resolution.proof {
        rootbeer_package::ResolutionProof::Catalog(proof) => *proof.source_proof,
        proof => proof,
    };
    let mut upstream = resolution.package;
    let realized = realizer
        .realize(&upstream)
        .map_err(|error| error.to_string())?;
    upstream.output_sha256 = Some(realized.store_entry.output_sha256.clone());
    crate::checks::check_package(
        &upstream,
        &realized,
        &realizer,
        &recipe.checks,
        staging.path(),
        options,
    )?;
    if options.environment_identity(catalog, request, context)? != environment {
        return Err("package environment changed during qualification".into());
    }
    let archive = destination.join("package.tar.gz");
    rootbeer_build::pack(&realized.store_entry.path, &archive)
        .map_err(|error| error.to_string())?;
    let mut package = upstream.clone();
    package.source = LockedSource::File {
        path: output.join("package.tar.gz"),
        sha256: hash_file(&archive).map_err(|error| error.to_string())?,
    };
    package.install = LockedInstall::Archive {
        format: ArchiveFormat::TarGz,
        strip_prefix: None,
    };
    let receipt = BinaryReceipt {
        schema: 1,
        system: ResolveContext::current().system,
        recipe_sha256: recipe.sha256(),
        package: package.clone(),
        provenance: UpstreamProvenance {
            engine_sha256: rootbeer_build::engine_identity(None),
            environment_sha256: environment,
            upstream,
            resolution: proof,
            resolver_inputs: inputs,
        },
    };
    crate::publication::write_json(&destination.join("receipt.json"), &receipt)?;
    fs::rename(destination, output).map_err(|error| error.to_string())?;
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use rootbeer_package::distribution::{verify_record, PackageProvenance};
    use rootbeer_package::{
        download::DownloadCache, Provides, ResolutionProof, SnapshotProof, SnapshotSource,
    };
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn qualifies_and_signs_upstream_binaries_without_source_build_evidence() {
        for is_archive in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let tree = root.path().join("upstream");
            fs::create_dir_all(tree.join("bin")).unwrap();
            let executable = tree.join("bin/demo");
            fs::write(&executable, b"#!/bin/sh\n[ \"$1\" = --version ]\n").unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
            let (source, install) = if is_archive {
                let archive = root.path().join("upstream.tar.gz");
                rootbeer_build::pack(&tree, &archive).unwrap();
                (
                    archive,
                    LockedInstall::Archive {
                        format: ArchiveFormat::TarGz,
                        strip_prefix: None,
                    },
                )
            } else {
                (
                    executable,
                    LockedInstall::Binary {
                        path: "bin/demo".into(),
                    },
                )
            };
            let options = BuildOptions {
                downloads: root.path().join("downloads"),
                cache: Some(crate::BuildCache {
                    directory: root.path().join("cache"),
                    context: "fixture-image".into(),
                    recheck: false,
                }),
                ..Default::default()
            };
            let cached = DownloadCache::new(&options.downloads)
                .materialize(&format!("file://{}", source.display()), None)
                .unwrap();
            let system = ResolveContext::current().system;
            let catalog: PackageCatalog = serde_json::from_value(serde_json::json!({
                "schema": 1, "packages": {"demo": {
                    "name": "demo", "description": "Binary qualification fixture", "homepage": "https://example.com",
                    "default_version": "1", "versions": {"1": {
                        "revision": 1, "source": "github:example/demo@v1",
                        "systems": [system], "assets": {system.clone(): "demo"},
                        "checksums": {system.clone(): cached.sha256},
                        "bins": ["demo"], "checks": [["demo", "--version"]]
                    }}
                }}
            })).unwrap();
            let task =
                crate::plan_packages(&catalog, &["demo@1".into()], &options, "fixture-image")
                    .unwrap()
                    .remove(0);
            let package = LockedPackage {
                name: "demo".into(),
                version: "1".into(),
                source: LockedSource::Url {
                    url: "https://example.com/demo".into(),
                    sha256: cached.sha256,
                },
                install,
                provides: Provides {
                    bins: BTreeMap::from([("demo".into(), "bin/demo".into())]),
                    apps: BTreeMap::new(),
                },
                output_sha256: None,
                runtime_dependencies: BTreeMap::new(),
            };
            let resolution = PackageResolution::new(
                package,
                ResolutionProof::Snapshot(SnapshotProof {
                    resolver: "fixture".into(),
                    source: SnapshotSource::Url {
                        url: "https://example.com/metadata.json".into(),
                    },
                    documents: vec![],
                }),
            );
            let prepared = root.path().join("prepared");
            prepare_binary(
                &catalog,
                "demo@1",
                &prepared,
                &options,
                resolution.clone(),
                PackageResolverInputs::default(),
            )
            .unwrap();
            let key = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
            let public_key: String = Ed25519KeyPair::from_pkcs8(key.as_ref())
                .unwrap()
                .public_key()
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let release = root.path().join("release");
            crate::release_package(
                &catalog.packages["demo"],
                &prepared.join("receipt.json"),
                "example/demo",
                &release,
                key.as_ref(),
                &public_key,
                Some(&task.key),
            )
            .unwrap();
            let bytes = fs::read(release.join("package.json")).unwrap();
            let record = verify_record(&bytes, &public_key, "demo@1", &system).unwrap();
            assert_eq!(record.input_key(), task.key);
            let PackageProvenance::Upstream(provenance) = &record.provenance else {
                panic!("binary claimed a source build")
            };
            assert_eq!(provenance.upstream.source, resolution.package.source);
            assert_eq!(
                provenance.upstream.output_sha256,
                record.artifact.package.output_sha256
            );
            assert!(!String::from_utf8(bytes).unwrap().contains("toolchain"));
            let mut changed = record.clone();
            changed
                .recipe
                .checksums
                .insert(system.clone(), "f".repeat(64));
            assert!(changed.validate().unwrap_err().contains("checksum"));
            changed = record.clone();
            changed.artifact.package.output_sha256 = Some("f".repeat(64));
            assert!(changed.validate().unwrap_err().contains("upstream output"));
            assert!(crate::release_package(
                &catalog.packages["demo"],
                &prepared.join("receipt.json"),
                "example/demo",
                &root.path().join("wrong-inputs"),
                key.as_ref(),
                &public_key,
                Some(&"f".repeat(64))
            )
            .is_err());
            let mut failing = catalog.clone();
            failing
                .packages
                .get_mut("demo")
                .unwrap()
                .versions
                .get_mut("1")
                .unwrap()
                .checks = vec![vec!["demo".into(), "fail".into()]];
            assert!(prepare_binary(
                &failing,
                "demo@1",
                &root.path().join("failed"),
                &options,
                resolution,
                PackageResolverInputs::default()
            )
            .is_err());
            assert!(!root.path().join("failed").exists());
            fs::write(prepared.join("package.tar.gz"), b"tampered").unwrap();
            assert!(crate::release_package(
                &catalog.packages["demo"],
                &prepared.join("receipt.json"),
                "example/demo",
                &root.path().join("tampered"),
                key.as_ref(),
                &public_key,
                Some(&task.key)
            )
            .unwrap_err()
            .contains("hash mismatch"));
        }
    }
}
