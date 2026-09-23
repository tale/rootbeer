//! The documents a PDR publishes: a signed root, one immutable document per package, and the
//! signed records those documents name.
//!
//! Only the root is signed. Package documents and records are addressed by digest, beside the
//! root (`packages/<digest>.json`, `records/<digest>.json`), so trust never depends on where
//! the bytes were fetched from and a mirror is a copy of the directory.

use std::collections::{BTreeMap, BTreeSet};

use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};

use crate::catalog::valid_name;
use crate::index::is_sha256;
use crate::CatalogRecipe;
use rootbeer_catalog::decode_hex;

/// The root schema this build reads. A root above it needs a newer rb.
pub const ROOT_SCHEMA: u32 = 3;
pub const ROOT_LIMIT: usize = 4 * 1024 * 1024;
pub const DOCUMENT_LIMIT: usize = 1024 * 1024;
const SIGNING_TAG: &str = "rootbeer-pdr-v3";

/// Everything search and listing need, plus a digest for each package's detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    pub schema: u32,
    pub sequence: u64,
    pub packages: BTreeMap<String, RootPackage>,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootPackage {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub description: String,
    pub homepage: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_engine_level: Option<u32>,
    /// Unix seconds of this package's earliest and latest published record.
    pub added: u64,
    pub updated: u64,
    /// Only platforms with a published record for `version`.
    pub platforms: BTreeMap<String, RootPlatform>,
    /// Digest of this package's document.
    pub document: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootPlatform {
    pub version: String,
    pub kind: PackageKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Command,
    Library,
    App,
}

/// Every retained version of one package and what each platform installs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDocument {
    pub name: String,
    pub versions: BTreeMap<String, DocumentVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub license: String,
    pub revision: u32,
    /// Only platforms with a published record.
    pub platforms: BTreeMap<String, DocumentPlatform>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentPlatform {
    pub recipe: CatalogRecipe,
    /// Digest of the signed record approving this recipe.
    pub record: String,
    /// Unix seconds, copied from the record.
    pub published: u64,
}

/// Bytes the publisher signs: every package entry, including fields this build does not know.
pub fn signing_message(sequence: u64, packages: &impl Serialize) -> Result<Vec<u8>, String> {
    rootbeer_catalog::canonical_json(&(SIGNING_TAG, sequence, packages))
        .map_err(|error| error.to_string())
}

/// Verifies a root's signature over the document as published, without decoding its packages.
///
/// Verification runs on the raw JSON so fields a newer publisher added stay covered by the
/// signature even though this build ignores them, and so a caller can decode only the entry it
/// needs when another package uses something this build cannot read.
pub fn verify_root(bytes: &[u8], public_key: &str) -> Result<serde_json::Value, String> {
    if bytes.len() > ROOT_LIMIT {
        return Err("PDR root exceeds 4 MiB".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let schema = value["schema"].as_u64().ok_or("PDR root has no schema")?;
    if schema > u64::from(ROOT_SCHEMA) {
        return Err(format!(
            "this package repository needs a newer rb (schema {schema}); run rb self-update"
        ));
    }
    if schema != u64::from(ROOT_SCHEMA) {
        return Err(format!("unsupported PDR root schema {schema}"));
    }
    let sequence = value["sequence"]
        .as_u64()
        .ok_or("PDR root has no sequence")?;
    let signature = value["signature"]
        .as_str()
        .ok_or("PDR root has no signature")?;
    UnparsedPublicKey::new(&ED25519, decode_hex::<32>(public_key)?)
        .verify(
            &signing_message(sequence, &value["packages"])?,
            &decode_hex::<64>(signature)?,
        )
        .map_err(|_| "PDR root signature verification failed".to_string())?;
    Ok(value)
}

impl Root {
    /// Verifies the signature over the document as published, then decodes it.
    pub fn from_bytes(bytes: &[u8], public_key: &str) -> Result<Self, String> {
        let root: Self = serde_json::from_value(verify_root(bytes, public_key)?)
            .map_err(|error| error.to_string())?;
        root.validate()?;
        Ok(root)
    }

    pub fn signing_message(&self) -> Result<Vec<u8>, String> {
        signing_message(self.sequence, &self.packages)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != ROOT_SCHEMA || self.sequence == 0 {
            return Err("invalid PDR root schema or sequence".into());
        }
        let mut names = BTreeSet::new();
        for (name, package) in &self.packages {
            for identity in std::iter::once(name).chain(&package.aliases) {
                if !valid_name(identity) || !names.insert(identity) {
                    return Err(format!(
                        "{name}: invalid or duplicate package name `{identity}`"
                    ));
                }
            }
            package
                .validate()
                .map_err(|error| format!("{name}: {error}"))?;
        }
        Ok(())
    }

    /// Resolves a canonical name or alias.
    pub fn find(&self, name: &str) -> Option<(&str, &RootPackage)> {
        self.packages
            .get_key_value(name)
            .or_else(|| {
                self.packages
                    .iter()
                    .find(|(_, package)| package.aliases.iter().any(|alias| alias == name))
            })
            .map(|(name, package)| (name.as_str(), package))
    }
}

impl RootPackage {
    pub fn validate(&self) -> Result<(), String> {
        if self.description.trim().is_empty()
            || !self.homepage.starts_with("https://")
            || self.license.trim().is_empty()
        {
            return Err("invalid package metadata".into());
        }
        if self.platforms.is_empty() {
            return Err("a published package has at least one platform".into());
        }
        if self.added == 0 || self.added > self.updated {
            return Err("invalid publication times".into());
        }
        if !is_sha256(&self.document) {
            return Err("package document requires a lowercase SHA-256".into());
        }
        crate::catalog::validate_systems(&self.platforms.keys().cloned().collect::<Vec<_>>())?;
        for (system, platform) in &self.platforms {
            if platform.version.is_empty()
                || platform
                    .commands
                    .iter()
                    .any(|command| !valid_command(command))
                || (platform.kind == PackageKind::Command) == platform.commands.is_empty()
            {
                return Err(format!("{system}: invalid platform entry"));
            }
        }
        Ok(())
    }

    /// Checks a fetched document against what this root promised about it.
    ///
    /// The digest already binds the bytes; this rejects a document that is well-formed but
    /// was pinned by mistake, such as another package's or one missing a listed platform.
    pub fn check_document(&self, name: &str, document: &PackageDocument) -> Result<(), String> {
        document.validate()?;
        if document.name != name {
            return Err(format!("{name}: document belongs to `{}`", document.name));
        }
        for (system, platform) in &self.platforms {
            let published = document
                .versions
                .get(&platform.version)
                .and_then(|version| version.platforms.get(system));
            if published.is_none() {
                return Err(format!(
                    "{name}@{} has no published {system} entry in its document",
                    platform.version
                ));
            }
        }
        Ok(())
    }
}

impl PackageDocument {
    /// Decodes a document whose bytes the caller already matched against the root's digest.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > DOCUMENT_LIMIT {
            return Err("package document exceeds 1 MiB".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !valid_name(&self.name) || self.versions.is_empty() {
            return Err("invalid package document identity".into());
        }
        for (version, entry) in &self.versions {
            let id = format!("{}@{version}", self.name);
            if version.is_empty() || entry.license.trim().is_empty() || entry.platforms.is_empty() {
                return Err(format!("{id}: invalid version entry"));
            }
            for (system, platform) in &entry.platforms {
                crate::catalog::validate_systems(std::slice::from_ref(system))?;
                platform
                    .recipe
                    .validate(system)
                    .map_err(|error| format!("{id} {system}: {error}"))?;
                if !is_sha256(&platform.record) || platform.published == 0 {
                    return Err(format!("{id} {system}: invalid record reference"));
                }
            }
        }
        Ok(())
    }
}

fn valid_command(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn recipe() -> CatalogRecipe {
        serde_json::from_value(serde_json::json!({
            "source": "github:owner/fd@v10.5.0",
            "asset": "fd.tar.gz",
            "sha256": "a".repeat(64),
            "bins": ["fd"],
            "checks": [["fd", "--version"]],
        }))
        .unwrap()
    }

    fn document() -> PackageDocument {
        PackageDocument {
            name: "fd".into(),
            versions: BTreeMap::from([(
                "10.5.0".into(),
                DocumentVersion {
                    license: "MIT".into(),
                    revision: 1,
                    platforms: BTreeMap::from([(
                        "aarch64-macos".into(),
                        DocumentPlatform {
                            recipe: recipe(),
                            record: "b".repeat(64),
                            published: 100,
                        },
                    )]),
                },
            )]),
        }
    }

    fn root() -> Root {
        Root {
            schema: ROOT_SCHEMA,
            sequence: 1,
            packages: BTreeMap::from([(
                "fd".into(),
                RootPackage {
                    aliases: vec!["fdfind".into()],
                    description: "Find entries".into(),
                    homepage: "https://example.com".into(),
                    license: "MIT".into(),
                    maintainers: vec!["tale".into()],
                    min_engine_level: None,
                    added: 100,
                    updated: 100,
                    platforms: BTreeMap::from([(
                        "aarch64-macos".into(),
                        RootPlatform {
                            version: "10.5.0".into(),
                            kind: PackageKind::Command,
                            commands: vec!["fd".into()],
                        },
                    )]),
                    document: "c".repeat(64),
                },
            )]),
            signature: String::new(),
        }
    }

    /// Signs `value` as a publisher would, whatever fields it carries.
    fn sign(mut value: serde_json::Value) -> (Vec<u8>, String) {
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let sequence = value["sequence"].as_u64().unwrap();
        let message = signing_message(sequence, &value["packages"]).unwrap();
        value["signature"] = hex(key.sign(&message).as_ref()).into();
        (
            serde_json::to_vec(&value).unwrap(),
            hex(key.public_key().as_ref()),
        )
    }

    #[test]
    fn a_signed_root_round_trips() {
        let (bytes, key) = sign(serde_json::to_value(root()).unwrap());
        let decoded = Root::from_bytes(&bytes, &key).unwrap();
        assert_eq!(decoded.packages, root().packages);
        assert_eq!(decoded.find("fdfind").unwrap().0, "fd");
        assert!(Root::from_bytes(&bytes, &"f".repeat(64)).is_err());
    }

    #[test]
    fn fields_this_build_does_not_know_stay_under_the_signature() {
        let mut value = serde_json::to_value(root()).unwrap();
        value["packages"]["fd"]["funding"] = "https://example.com/sponsor".into();
        let (bytes, key) = sign(value);
        assert!(
            Root::from_bytes(&bytes, &key).is_ok(),
            "unknown fields are ignored"
        );

        let tampered = String::from_utf8(bytes)
            .unwrap()
            .replace("example.com/sponsor", "example.com/phish");
        let error = Root::from_bytes(tampered.as_bytes(), &key).unwrap_err();
        assert!(error.contains("signature"), "{error}");
    }

    #[test]
    fn a_newer_root_asks_for_a_newer_rb() {
        let mut value = serde_json::to_value(root()).unwrap();
        value["schema"] = 4.into();
        let (bytes, key) = sign(value);
        let error = Root::from_bytes(&bytes, &key).unwrap_err();
        assert!(error.contains("rb self-update"), "{error}");
    }

    #[test]
    fn an_alias_cannot_shadow_another_package() {
        let mut root = root();
        let mut other = root.packages["fd"].clone();
        other.aliases = vec!["fd".into()];
        root.packages.insert("find".into(), other);
        assert!(root.validate().unwrap_err().contains("duplicate"));
    }

    #[test]
    fn a_platform_lists_commands_exactly_when_it_is_a_command() {
        let mut root = root();
        let platform = root
            .packages
            .get_mut("fd")
            .unwrap()
            .platforms
            .get_mut("aarch64-macos")
            .unwrap();
        platform.kind = PackageKind::App;
        assert!(root.validate().is_err());
    }

    #[test]
    fn a_document_must_carry_every_platform_the_root_lists() {
        let root = root();
        let package = &root.packages["fd"];
        assert!(package.check_document("fd", &document()).is_ok());
        assert!(package
            .check_document("bat", &document())
            .unwrap_err()
            .contains("belongs to"));

        let mut listed = package.clone();
        listed.platforms.insert(
            "x86_64-linux".into(),
            listed.platforms["aarch64-macos"].clone(),
        );
        let error = listed.check_document("fd", &document()).unwrap_err();
        assert!(error.contains("x86_64-linux"), "{error}");
    }

    #[test]
    fn a_document_names_its_records_by_digest() {
        let mut document = document();
        document
            .versions
            .get_mut("10.5.0")
            .unwrap()
            .platforms
            .get_mut("aarch64-macos")
            .unwrap()
            .record = "https://example.com/records/b.json".into();
        assert!(document.validate().is_err());
        let bytes = serde_json::to_vec(&document).unwrap();
        assert!(PackageDocument::from_bytes(&bytes).is_err());
    }
}
