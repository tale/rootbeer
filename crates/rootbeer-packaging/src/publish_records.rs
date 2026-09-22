use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ring::signature::{Ed25519KeyPair, KeyPair};
use rootbeer_package::distribution::{read_record, verify_record, SignedPackageRecord};
use rootbeer_package::pdr::{
    DocumentPlatform, DocumentVersion, PackageDocument, PackageKind, Root, RootPackage,
    RootPlatform, ROOT_SCHEMA,
};
use rootbeer_package::{CatalogPackage, CatalogRecipe, PackageCatalog};
use rootbeer_store::hash_bytes;

/// Package, version and system.
type Key = (String, String, String);

/// A signed record as the documents describe it.
#[derive(Debug, Clone, PartialEq)]
struct Published {
    recipe: CatalogRecipe,
    record: String,
    published: u64,
}

/// Publishes the signed root and package documents for every approved record, adding new
/// records to those the previous root already carried.
///
/// Nothing is downloaded or rebuilt. A previous record survives only while the catalog still
/// approves exactly its recipe and revision, so a recipe change unpublishes that platform until
/// a new record for it arrives.
pub fn publish_records(
    catalog: &PackageCatalog,
    references: &[String],
    site: &Path,
    key_der: &[u8],
    public_key: &str,
) -> Result<usize, String> {
    catalog.validate()?;
    let key =
        Ed25519KeyPair::from_pkcs8(key_der).map_err(|_| "invalid Ed25519 PKCS#8 signing key")?;
    if key.public_key().as_ref() != rootbeer_package::official::decode_hex::<32>(public_key)? {
        return Err("signing key does not match the PDR verification key".into());
    }

    let current = site.join("current.json");
    let previous = if current.exists() {
        let bytes = fs::read(&current).map_err(|error| error.to_string())?;
        Some(Root::from_bytes(&bytes, public_key)?)
    } else {
        None
    };
    let mut published = match &previous {
        Some(previous) => retained(catalog, &previous_documents(previous, site)?),
        None => BTreeMap::new(),
    };
    for entry in published.values() {
        let path = site.join("records").join(format!("{}.json", entry.record));
        if !path.is_file() {
            return Err(format!(
                "retained record {} is missing from the site",
                entry.record
            ));
        }
    }

    let mut released: BTreeMap<Key, (Published, Vec<u8>)> = BTreeMap::new();
    for reference in references {
        let bytes = read_record(reference)?;
        let (key, entry) = approve(catalog, &bytes, public_key)?;
        if released
            .get(&key)
            .is_some_and(|(other, _)| other.record != entry.record)
        {
            return Err(format!(
                "{}@{} {}: conflicting records in one publication",
                key.0, key.1, key.2
            ));
        }
        released.insert(key, (entry, bytes));
    }
    for (key, (entry, _)) in &released {
        published.insert(key.clone(), entry.clone());
    }
    if published.is_empty() {
        return Err("a PDR publishes at least one record".into());
    }

    let sequence = match &previous {
        Some(previous) => previous
            .sequence
            .checked_add(1)
            .ok_or("PDR sequence overflow")?,
        None => 1,
    };
    let (mut root, documents) = assemble(catalog, previous.as_ref(), &published, sequence)?;
    root.signature = key
        .sign(&root.signing_message()?)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let bytes = serde_json::to_vec(&root).map_err(|error| error.to_string())?;
    Root::from_bytes(&bytes, public_key)?;

    for directory in ["records", "packages", "roots"] {
        fs::create_dir_all(site.join(directory)).map_err(|error| error.to_string())?;
    }
    for (entry, record) in released.values() {
        fs::write(
            site.join("records").join(format!("{}.json", entry.record)),
            record,
        )
        .map_err(|error| error.to_string())?;
    }
    for (digest, document) in &documents {
        fs::write(
            site.join("packages").join(format!("{digest}.json")),
            document,
        )
        .map_err(|error| error.to_string())?;
    }
    fs::write(
        site.join("roots")
            .join(format!("{}.json", hash_bytes(&bytes))),
        &bytes,
    )
    .map_err(|error| error.to_string())?;
    let temporary = site.join("current.json.tmp");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(temporary, current).map_err(|error| error.to_string())?;
    Ok(root
        .packages
        .values()
        .map(|package| package.platforms.len())
        .sum())
}

/// Verifies one released record against the catalog that is about to publish it.
fn approve(
    catalog: &PackageCatalog,
    bytes: &[u8],
    public_key: &str,
) -> Result<(Key, Published), String> {
    let signed: SignedPackageRecord =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let claimed: rootbeer_package::distribution::PackageRecord =
        serde_json::from_str(signed.record.get()).map_err(|error| error.to_string())?;
    let id = claimed.artifact.package.id();
    let record = verify_record(bytes, public_key, &id, &claimed.system)?;
    let package = &record.artifact.package;
    let version = catalog
        .packages
        .get(&package.name)
        .and_then(|entry| entry.versions.get(&package.version))
        .ok_or_else(|| format!("{id}: not in the catalog"))?;
    if version.platforms.get(&record.system) != Some(&record.recipe)
        || version.revision != record.revision
    {
        return Err(format!(
            "{id} {}: signed record differs from the approved recipe",
            record.system
        ));
    }
    Ok((
        (
            package.name.clone(),
            package.version.clone(),
            record.system.clone(),
        ),
        Published {
            recipe: record.recipe.clone(),
            record: hash_bytes(bytes),
            published: record.published,
        },
    ))
}

/// Reads the documents the previous root pins, checking each against its digest.
fn previous_documents(
    previous: &Root,
    site: &Path,
) -> Result<BTreeMap<String, PackageDocument>, String> {
    let mut documents = BTreeMap::new();
    for (name, package) in &previous.packages {
        let path = site
            .join("packages")
            .join(format!("{}.json", package.document));
        let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if hash_bytes(&bytes) != package.document {
            return Err(format!(
                "{name}: package document does not match its digest"
            ));
        }
        let document = PackageDocument::from_bytes(&bytes)?;
        package.check_document(name, &document)?;
        documents.insert(name.clone(), document);
    }
    Ok(documents)
}

/// Previous records the catalog still approves exactly.
fn retained(
    catalog: &PackageCatalog,
    documents: &BTreeMap<String, PackageDocument>,
) -> BTreeMap<Key, Published> {
    let mut retained = BTreeMap::new();
    for (name, document) in documents {
        for (version, entry) in &document.versions {
            let approved = catalog
                .packages
                .get(name)
                .and_then(|package| package.versions.get(version));
            for (system, platform) in &entry.platforms {
                let is_unchanged = approved.is_some_and(|approved| {
                    approved.revision == entry.revision
                        && approved.platforms.get(system) == Some(&platform.recipe)
                });
                if !is_unchanged {
                    continue;
                }
                retained.insert(
                    (name.clone(), version.clone(), system.clone()),
                    Published {
                        recipe: platform.recipe.clone(),
                        record: platform.record.clone(),
                        published: platform.published,
                    },
                );
            }
        }
    }
    retained
}

/// Builds the unsigned root and every package document, keyed by digest.
///
/// A platform lists its authored default when that version is published. Otherwise the
/// version the previous root listed stands in while it is still published, so a pending
/// release never takes a working package away; with neither, the platform is absent.
fn assemble(
    catalog: &PackageCatalog,
    previous: Option<&Root>,
    published: &BTreeMap<Key, Published>,
    sequence: u64,
) -> Result<(Root, BTreeMap<String, Vec<u8>>), String> {
    let mut root = Root {
        schema: ROOT_SCHEMA,
        sequence,
        packages: BTreeMap::new(),
        signature: String::new(),
    };
    let mut documents = BTreeMap::new();
    for (name, package) in &catalog.packages {
        let document = document(package, published);
        if document.versions.is_empty() {
            continue;
        }
        let is_published = |system: &str, version: &str| {
            document
                .versions
                .get(version)
                .is_some_and(|entry| entry.platforms.contains_key(system))
        };
        let mut platforms = BTreeMap::new();
        for (system, authored) in &package.default_versions {
            let listed = previous
                .and_then(|previous| previous.packages.get(name))
                .and_then(|entry| entry.platforms.get(system))
                .map(|platform| platform.version.as_str());
            let Some(version) = [Some(authored.as_str()), listed]
                .into_iter()
                .flatten()
                .find(|version| is_published(system, version))
            else {
                continue;
            };
            let recipe = &document.versions[version].platforms[system].recipe;
            platforms.insert(system.clone(), platform(version, recipe));
        }
        if platforms.is_empty() {
            continue;
        }

        let mut licenses = platforms
            .values()
            .map(|platform| &document.versions[&platform.version].license);
        let license = licenses.next().expect("platforms is not empty").clone();
        if licenses.any(|other| *other != license) {
            return Err(format!(
                "{name}: default versions disagree on their license"
            ));
        }
        let times = document
            .versions
            .values()
            .flat_map(|entry| entry.platforms.values())
            .map(|platform| platform.published);
        let added = times.clone().min().expect("document is not empty");
        let updated = times.max().expect("document is not empty");

        let bytes =
            rootbeer_catalog::canonical_json(&document).map_err(|error| error.to_string())?;
        let digest = hash_bytes(&bytes);
        documents.insert(digest.clone(), bytes);
        root.packages.insert(
            name.clone(),
            RootPackage {
                aliases: package.aliases.clone(),
                description: package.description.clone(),
                homepage: package.homepage.clone(),
                license,
                maintainers: package.recipe_maintainers.clone(),
                min_engine_level: package.min_engine_level,
                added,
                updated,
                platforms,
                document: digest,
            },
        );
    }
    root.validate()?;
    Ok((root, documents))
}

fn document(package: &CatalogPackage, published: &BTreeMap<Key, Published>) -> PackageDocument {
    let mut versions = BTreeMap::new();
    for (version, approved) in &package.versions {
        let platforms: BTreeMap<_, _> = approved
            .platforms
            .keys()
            .filter_map(|system| {
                let key = (package.name.clone(), version.clone(), system.clone());
                let entry = published.get(&key)?;
                Some((
                    system.clone(),
                    DocumentPlatform {
                        recipe: entry.recipe.clone(),
                        record: entry.record.clone(),
                        published: entry.published,
                    },
                ))
            })
            .collect();
        if platforms.is_empty() {
            continue;
        }
        versions.insert(
            version.clone(),
            DocumentVersion {
                license: approved.license.clone(),
                revision: approved.revision,
                platforms,
            },
        );
    }
    PackageDocument {
        name: package.name.clone(),
        versions,
    }
}

fn platform(version: &str, recipe: &CatalogRecipe) -> RootPlatform {
    let commands: Vec<String> = recipe.bins.names().into_iter().cloned().collect();
    let kind = if !commands.is_empty() {
        PackageKind::Command
    } else if !recipe.apps.is_empty() {
        PackageKind::App
    } else {
        PackageKind::Library
    };
    RootPlatform {
        version: version.to_string(),
        kind,
        commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rootbeer_package::PackageDefinition;

    const MAC: &str = "aarch64-macos";
    const LINUX: &str = "x86_64-linux";

    /// `tool` builds versions 1 and 2 on macOS and Linux, defaulting to 2.
    fn catalog() -> PackageCatalog {
        let digest = "a".repeat(64);
        let definition = PackageDefinition::from_lua(&format!(
            r#"return {{
                name = "tool", description = "A tool", homepage = "https://example.com",
                default_license = "MIT",
                prebuilt = {{ github = "owner/tool", tag = "v{{version}}", asset = "tool-{{target}}.tar.gz" }},
                outputs = {{ bins = {{ "tool" }}, checks = {{ {{ "tool", "--version" }} }} }},
                platforms = {{
                    ["{MAC}"] = {{ target = "mac", default_version = "2" }},
                    ["{LINUX}"] = {{ target = "linux", default_version = "2" }},
                }},
                versions = {{
                    ["1"] = {{ digests = {{ ["{MAC}"] = "{digest}", ["{LINUX}"] = "{digest}" }} }},
                    ["2"] = {{ digests = {{ ["{MAC}"] = "{digest}", ["{LINUX}"] = "{digest}" }} }},
                }},
            }}"#
        ))
        .unwrap();
        PackageCatalog::from_definitions(&BTreeMap::from([("tool".into(), definition)])).unwrap()
    }

    fn key(version: &str, system: &str) -> Key {
        ("tool".into(), version.into(), system.into())
    }

    /// Records for `version` on each system, published at `time`.
    fn release(
        catalog: &PackageCatalog,
        version: &str,
        systems: &[&str],
        time: u64,
    ) -> BTreeMap<Key, Published> {
        let approved = &catalog.packages["tool"].versions[version];
        systems
            .iter()
            .map(|system| {
                (
                    key(version, system),
                    Published {
                        recipe: approved.platforms[*system].clone(),
                        record: hash_bytes(format!("{version} {system}").as_bytes()),
                        published: time,
                    },
                )
            })
            .collect()
    }

    fn documents_of(
        root: &Root,
        documents: &BTreeMap<String, Vec<u8>>,
    ) -> BTreeMap<String, PackageDocument> {
        root.packages
            .iter()
            .map(|(name, package)| {
                let document = PackageDocument::from_bytes(&documents[&package.document]).unwrap();
                package.check_document(name, &document).unwrap();
                (name.clone(), document)
            })
            .collect()
    }

    #[test]
    fn a_platform_lists_its_default_once_published_and_its_previous_version_until_then() {
        let catalog = catalog();
        let mut published = release(&catalog, "1", &[MAC, LINUX], 100);
        let (first, _) = assemble(&catalog, None, &published, 1).unwrap();
        assert!(
            first.packages.is_empty(),
            "version 1 was never the default and nothing listed it"
        );

        published.extend(release(&catalog, "2", &[MAC], 200));
        let (second, _) = assemble(&catalog, None, &published, 2).unwrap();
        let platforms = &second.packages["tool"].platforms;
        assert_eq!(platforms[MAC].version, "2");
        assert!(!platforms.contains_key(LINUX));

        let mut listed = second.clone();
        listed.packages.get_mut("tool").unwrap().platforms.insert(
            LINUX.into(),
            platform(
                "1",
                &catalog.packages["tool"].versions["1"].platforms[LINUX],
            ),
        );
        let (third, _) = assemble(&catalog, Some(&listed), &published, 3).unwrap();
        let platforms = &third.packages["tool"].platforms;
        assert_eq!(platforms[MAC].version, "2");
        assert_eq!(
            platforms[LINUX].version, "1",
            "Linux keeps what it had until its version 2 record arrives"
        );
    }

    #[test]
    fn documents_carry_every_published_version_and_its_times() {
        let catalog = catalog();
        let mut published = release(&catalog, "1", &[MAC, LINUX], 100);
        published.extend(release(&catalog, "2", &[MAC, LINUX], 200));
        let (root, documents) = assemble(&catalog, None, &published, 1).unwrap();

        let package = &root.packages["tool"];
        assert_eq!((package.added, package.updated), (100, 200));
        assert_eq!(package.platforms[MAC].kind, PackageKind::Command);
        assert_eq!(package.platforms[MAC].commands, ["tool"]);
        let document = &documents_of(&root, &documents)["tool"];
        assert_eq!(document.versions.len(), 2);
        assert_eq!(document.versions["1"].platforms[LINUX].published, 100);

        let (again, rebuilt) = assemble(&catalog, None, &published, 1).unwrap();
        assert_eq!(again, root);
        assert_eq!(
            rebuilt, documents,
            "unchanged inputs produce identical documents"
        );
    }

    #[test]
    fn a_changed_recipe_or_revision_unpublishes_only_that_platform() {
        let catalog = catalog();
        let published = release(&catalog, "2", &[MAC, LINUX], 100);
        let (root, documents) = assemble(&catalog, None, &published, 1).unwrap();
        let documents = documents_of(&root, &documents);
        assert_eq!(retained(&catalog, &documents), published);

        let mut changed = catalog.clone();
        let version = changed
            .packages
            .get_mut("tool")
            .unwrap()
            .versions
            .get_mut("2")
            .unwrap();
        version
            .platforms
            .get_mut(MAC)
            .unwrap()
            .checks
            .push(vec!["tool".into(), "--help".into()]);
        let kept = retained(&changed, &documents);
        assert!(!kept.contains_key(&key("2", MAC)));
        assert!(kept.contains_key(&key("2", LINUX)));

        changed
            .packages
            .get_mut("tool")
            .unwrap()
            .versions
            .get_mut("2")
            .unwrap()
            .revision = 2;
        assert!(retained(&changed, &documents).is_empty());
    }

    #[test]
    fn default_versions_must_agree_on_a_license() {
        let mut catalog = catalog();
        catalog
            .packages
            .get_mut("tool")
            .unwrap()
            .default_versions
            .insert(LINUX.into(), "1".into());
        catalog
            .packages
            .get_mut("tool")
            .unwrap()
            .versions
            .get_mut("1")
            .unwrap()
            .license = "Apache-2.0".into();
        let mut published = release(&catalog, "1", &[LINUX], 100);
        published.extend(release(&catalog, "2", &[MAC], 100));
        let error = assemble(&catalog, None, &published, 1).unwrap_err();
        assert!(error.contains("license"), "{error}");
    }
}
