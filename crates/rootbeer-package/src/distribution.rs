use std::collections::BTreeMap;
use std::io::Read;

use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::{BuildEnvironmentLock, CatalogRecipe, PublishedArtifact};

pub const RECORD_LIMIT: usize = 1024 * 1024;

/// Publisher-approved qualification evidence for one package, independent of a catalog or CI run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRecord {
    pub schema: u32,
    pub system: String,
    /// The catalog revision this qualification was produced for.
    pub revision: u32,
    pub recipe: CatalogRecipe,
    pub artifact: PublishedArtifact,
    pub provenance: PackageProvenance,
    #[serde(flatten, default, skip_serializing_if = "crate::ExtraFields::is_empty")]
    pub extra: crate::ExtraFields,
}

/// Distinguishes compilation evidence from an approved upstream binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PackageProvenance {
    Source(Box<BuildProvenance>),
    Upstream(Box<UpstreamProvenance>),
    Retained(Box<RetainedProvenance>),
}

impl PackageProvenance {
    fn qualification(&self) -> Option<(&str, &str)> {
        match self {
            Self::Source(provenance) => {
                Some((&provenance.engine_sha256, &provenance.environment_sha256))
            }
            Self::Upstream(provenance) => {
                Some((&provenance.engine_sha256, &provenance.environment_sha256))
            }
            Self::Retained(_) => None,
        }
    }
}

/// An existing signed catalog approval preserved when promoting its exact artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedProvenance {
    pub approval: crate::official::Manifest,
    pub receipt: crate::PackageIndexPin,
}

/// Exact upstream bytes and installation inputs approved by the publisher.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamProvenance {
    pub engine_sha256: String,
    pub environment_sha256: String,
    pub upstream: crate::LockedPackage,
    pub resolution: crate::ResolutionProof,
    pub resolver_inputs: crate::PackageResolverInputs,
}

/// Recorded build inputs; the publisher signature establishes approval, not builder identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProvenance {
    pub engine_sha256: String,
    pub environment_sha256: String,
    pub environment: BuildEnvironmentLock,
    pub isolation: String,
    pub toolchain: BTreeMap<String, String>,
    pub runtime_audit_sha256: String,
}

/// Signs the exact embedded JSON bytes, avoiding reserialization during verification.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPackageRecord {
    pub record: Box<RawValue>,
    pub signature: String,
}

impl PackageRecord {
    /// Identifies the exact qualified inputs, independently of publication time and storage.
    pub fn input_key(&self) -> String {
        if let Some((engine, environment)) = self.provenance.qualification() {
            return input_key(
                &self.artifact.package.id(),
                &self.system,
                self.revision,
                &self.recipe,
                engine,
                environment,
            );
        }
        crate::store::hash_bytes(
            &serde_json::to_vec(&(
                "rootbeer-retained-package-v1",
                &self.artifact.package.id(),
                &self.system,
                &self.recipe,
                &self.provenance,
            ))
            .expect("retained package inputs serialize"),
        )
    }

    /// Verifies any earlier publisher approval retained as package provenance.
    pub fn verify_provenance(&self, public_key: &str) -> Result<(), String> {
        if let PackageProvenance::Retained(provenance) = &self.provenance {
            crate::OfficialIndexSource {
                url: provenance.approval.index.url.clone(),
                public_key: public_key.into(),
            }
            .verify(&provenance.approval)?;
        }
        Ok(())
    }

    /// Validates the dependency-free package contract supported by this schema.
    pub fn validate(&self) -> Result<(), String> {
        let package = &self.artifact.package;
        if self.schema != 1 {
            return Err("unsupported package record schema".into());
        }
        if !crate::catalog::valid_name(&package.name)
            || package.version.is_empty()
            || !package
                .version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        {
            return Err("invalid package record identity".into());
        }
        self.recipe.validate(&self.system)?;
        if !matches!(self.provenance, PackageProvenance::Retained(_))
            && (self
                .recipe
                .build
                .as_ref()
                .is_some_and(|build| !build.dependencies.is_empty())
                || !package.runtime_dependencies.is_empty())
        {
            return Err("package records with dependencies are not supported yet".into());
        }
        if self
            .provenance
            .qualification()
            .is_some_and(|(engine, environment)| {
                !crate::index::is_sha256(engine) || !crate::index::is_sha256(environment)
            })
        {
            return Err("invalid package qualification identity".into());
        }
        match (&self.recipe.build, &self.provenance) {
            (_, PackageProvenance::Retained(provenance)) => {
                self.artifact
                    .validate(&package.id(), &self.system, self.revision, &self.recipe)?;
                crate::runtime::closure(package)?;
                provenance.approval.index.validate()?;
                provenance.receipt.validate()?;
                crate::index::validate_https(&provenance.receipt.url)?;
                if provenance.receipt.sha256 != self.artifact.receipt_sha256 {
                    return Err("retained approval has a different receipt".into());
                }
                Ok(())
            }
            (Some(_), PackageProvenance::Source(provenance)) => {
                self.artifact
                    .validate(&package.id(), &self.system, self.revision, &self.recipe)?;
                provenance.validate(&self.system)
            }
            (None, PackageProvenance::Upstream(provenance)) => {
                let mut recipe = self.recipe.clone();
                recipe.mirror = false;
                PublishedArtifact {
                    package: provenance.upstream.clone(),
                    ..self.artifact.clone()
                }
                .validate(&package.id(), &self.system, self.revision, &recipe)?;
                if !matches!(&provenance.upstream.source, crate::LockedSource::Url { url, .. } if url.starts_with("https://"))
                {
                    return Err("upstream evidence requires an HTTPS artifact".into());
                }
                recipe.mirror = true;
                self.artifact
                    .validate(&package.id(), &self.system, self.revision, &recipe)?;
                let mut expected = provenance.upstream.clone();
                expected.source = package.source.clone();
                expected.install = crate::LockedInstall::Archive {
                    format: crate::ArchiveFormat::TarGz,
                    strip_prefix: None,
                };
                if expected != *package {
                    return Err("repackaged artifact differs from its upstream output".into());
                }
                Ok(())
            }
            _ => Err("package provenance does not match its recipe kind".into()),
        }
    }
}

impl BuildProvenance {
    fn validate(&self, system: &str) -> Result<(), String> {
        let provenance = self;
        let environment = &provenance.environment;
        if environment.schema != 1
            || !crate::index::is_sha256(&provenance.engine_sha256)
            || !crate::index::is_sha256(&provenance.environment_sha256)
            || environment.system != system
            || ["sh", "cc", "make", "patch"]
                .iter()
                .any(|name| !environment.tools.contains_key(*name))
            || environment
                .tools
                .values()
                .chain(environment.inputs.values())
                .any(|input| !input.path.is_absolute() || !crate::index::is_sha256(&input.sha256))
            || provenance.isolation.is_empty()
            || provenance.toolchain.is_empty()
            || !crate::index::is_sha256(&provenance.runtime_audit_sha256)
        {
            return Err("invalid package build evidence".into());
        }
        Ok(())
    }
}

/// Content identity shared by package planning and signed-result lookup.
pub fn input_key(
    package: &str,
    system: &str,
    revision: u32,
    recipe: &CatalogRecipe,
    engine: &str,
    environment: &str,
) -> String {
    crate::store::hash_bytes(
        &serde_json::to_vec(&(
            "rootbeer-package-inputs-v1",
            package,
            system,
            revision,
            recipe,
            engine,
            environment,
        ))
        .expect("package inputs serialize"),
    )
}

/// Domain-separated bytes shared by package signing and verification.
pub fn signing_message(record: &RawValue) -> Vec<u8> {
    [b"rootbeer-package-v1\0".as_slice(), record.get().as_bytes()].concat()
}

/// Reads a local record or an immutable GHCR blob, bounding memory and checking its address.
pub fn read_record(reference: &str) -> Result<Vec<u8>, String> {
    let blob = if reference.starts_with("ghcr://") {
        Some(crate::ghcr::GhcrBlob::parse(reference)?)
    } else if reference.contains("://") {
        return Err("record must be a local file or an immutable ghcr:// reference".into());
    } else {
        None
    };
    let reader: Box<dyn Read> = match &blob {
        Some(blob) => blob.reader().map_err(|error| error.to_string())?,
        None => Box::new(std::fs::File::open(reference).map_err(|error| error.to_string())?),
    };
    let mut bytes = Vec::new();
    reader
        .take(RECORD_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > RECORD_LIMIT {
        return Err("package record exceeds 1 MiB".into());
    }
    if blob.is_some_and(|blob| blob.sha256 != crate::store::hash_bytes(&bytes)) {
        return Err("package record digest does not match its address".into());
    }
    Ok(bytes)
}

/// Verifies publisher approval and the exact requested identity before returning install inputs.
pub fn verify_record(
    bytes: &[u8],
    public_key: &str,
    package: &str,
    system: &str,
) -> Result<PackageRecord, String> {
    if bytes.len() > RECORD_LIMIT {
        return Err("package record exceeds 1 MiB".into());
    }
    let signed: SignedPackageRecord =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    UnparsedPublicKey::new(&ED25519, crate::official::decode_hex::<32>(public_key)?)
        .verify(
            &signing_message(&signed.record),
            &crate::official::decode_hex::<64>(&signed.signature)?,
        )
        .map_err(|_| "package signature verification failed".to_string())?;
    let record: PackageRecord =
        serde_json::from_str(signed.record.get()).map_err(|error| error.to_string())?;
    record.validate()?;
    record.verify_provenance(public_key)?;
    if record.artifact.package.id() != package || record.system != system {
        return Err(format!(
            "package record does not match {package} for {system}"
        ));
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn signed() -> (Vec<u8>, String, String) {
        let mut index = crate::artifact::fixture();
        let artifact = index
            .artifacts
            .values_mut()
            .next()
            .unwrap()
            .remove("aarch64-linux")
            .unwrap();
        let package = &artifact.package;
        let id = package.id();
        let record = PackageRecord {
            extra: Default::default(),
            schema: 1,
            system: "aarch64-linux".into(),
            revision: index.catalog.packages[&package.name].versions[&package.version].revision,
            recipe: index.catalog.packages[&package.name].versions[&package.version]
                .for_system("aarch64-linux")
                .unwrap()
                .clone(),
            artifact,
            provenance: PackageProvenance::Source(Box::new(BuildProvenance {
                engine_sha256: "a".repeat(64),
                environment_sha256: "b".repeat(64),
                environment: BuildEnvironmentLock {
                    schema: 1,
                    system: "aarch64-linux".into(),
                    tools: ["sh", "cc", "make", "patch"]
                        .into_iter()
                        .map(|name| {
                            (
                                name.into(),
                                crate::BuildEnvironmentInput {
                                    path: format!("/usr/bin/{name}").into(),
                                    sha256: "d".repeat(64),
                                },
                            )
                        })
                        .collect(),
                    inputs: BTreeMap::new(),
                    variables: BTreeMap::new(),
                },
                isolation: "host".into(),
                toolchain: BTreeMap::from([("cc".into(), "test compiler".into())]),
                runtime_audit_sha256: "e".repeat(64),
            })),
        };
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let record = serde_json::value::to_raw_value(&record).unwrap();
        let hex = |bytes: &[u8]| {
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        let signature = hex(key.sign(&signing_message(&record)).as_ref());
        let bytes = serde_json::to_vec(&SignedPackageRecord { record, signature }).unwrap();
        (bytes, hex(key.public_key().as_ref()), id)
    }

    #[test]
    fn approval_is_bound_to_exact_bytes_identity_platform_and_key() {
        let (bytes, key, id) = signed();
        let record = verify_record(&bytes, &key, &id, "aarch64-linux").unwrap();
        assert_eq!(record.artifact.package.id(), id);
        assert!(verify_record(&bytes, &"f".repeat(64), &id, "aarch64-linux").is_err());
        assert!(verify_record(&bytes, &key, "another@1", "aarch64-linux").is_err());
        assert!(verify_record(&bytes, &key, &id, "aarch64-macos").is_err());

        for (from, to) in [
            ("host", "sandbox"),
            ("aarch64-linux", "x86_64-linux"),
            ("test compiler", "another compiler"),
        ] {
            let changed = String::from_utf8(bytes.clone()).unwrap().replace(from, to);
            assert!(
                verify_record(changed.as_bytes(), &key, &id, "aarch64-linux")
                    .unwrap_err()
                    .contains("signature")
            );
        }
        let mut signed: SignedPackageRecord = serde_json::from_slice(&bytes).unwrap();
        let mut payload: serde_json::Value = serde_json::from_str(signed.record.get()).unwrap();
        payload["unrecognized"] = true.into();
        signed.record = serde_json::value::to_raw_value(&payload).unwrap();
        assert!(verify_record(
            &serde_json::to_vec(&signed).unwrap(),
            &key,
            &id,
            "aarch64-linux"
        )
        .is_err());
    }

    #[test]
    fn records_reject_dependencies_and_missing_build_evidence() {
        let (bytes, key, id) = signed();
        let mut record = verify_record(&bytes, &key, &id, "aarch64-linux").unwrap();
        record
            .recipe
            .build
            .as_mut()
            .unwrap()
            .dependencies
            .push("dependency@1".into());
        assert!(record.validate().unwrap_err().contains("dependencies"));
        record.recipe.build.as_mut().unwrap().dependencies.clear();
        let PackageProvenance::Source(provenance) = &mut record.provenance else {
            panic!()
        };
        provenance.environment.tools.clear();
        assert!(record.validate().is_err());
    }

    #[test]
    fn input_keys_bind_recipe_checks_engine_environment_and_platform() {
        let (bytes, key, id) = signed();
        let record = verify_record(&bytes, &key, &id, "aarch64-linux").unwrap();
        let expected = record.input_key();
        for change in 0..5 {
            let mut changed = record.clone();
            let PackageProvenance::Source(provenance) = &mut changed.provenance else {
                panic!()
            };
            match change {
                0 => changed.recipe.checks.push(vec!["additional-check".into()]),
                1 => provenance.engine_sha256 = "c".repeat(64),
                2 => provenance.environment_sha256 = "c".repeat(64),
                3 => changed.system = "x86_64-linux".into(),
                _ => changed.artifact.package.version.push_str(".1"),
            }
            assert_ne!(expected, changed.input_key());
        }
        let mut republished = record.clone();
        republished.artifact.receipt_sha256 = "f".repeat(64);
        assert_eq!(expected, republished.input_key());
    }
}
