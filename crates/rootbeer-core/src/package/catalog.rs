use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use mlua::{Lua, LuaSerdeExt};
use serde::{Deserialize, Serialize};

use super::{
    PackageRequest, PackageRequestResolver, PackageResolution, PackageResolver,
    PackageResolverInputs, ResolutionProof, ResolveContext, ResolverStack,
};
use crate::store::hash_bytes;

include!(concat!(env!("OUT_DIR"), "/package_catalog.rs"));

/// A versioned snapshot of Rootbeer's canonical package definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCatalog {
    pub schema: u32,
    pub packages: BTreeMap<String, CatalogPackage>,
}

/// The upstream identity and approved versions of a canonical package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackage {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    pub homepage: String,
    pub default_version: String,
    pub versions: BTreeMap<String, CatalogRecipe>,
}

/// An explicit backend request and its platform and command contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRecipe {
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<super::SourceBuild>,
    pub systems: Vec<String>,
    pub bins: Vec<String>,
    pub checks: Vec<Vec<String>>,
}

/// Connects canonical identity to the exact backend resolution used to install it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogProof {
    pub catalog_sha256: String,
    pub name: String,
    pub version: String,
    pub revision: u32,
    pub source: PackageRequest,
    pub source_proof: Box<ResolutionProof>,
}

impl PackageCatalog {
    /// Loads and validates the collection shipped with this binary without I/O.
    pub fn embedded() -> Result<&'static Self, String> {
        static CATALOG: OnceLock<Result<PackageCatalog, String>> = OnceLock::new();
        CATALOG
            .get_or_init(|| Self::from_lua(EMBEDDED_RECIPES))
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Loads a recipe directory without rebuilding the executable.
    pub fn from_directory(directory: &Path) -> Result<Self, String> {
        let mut recipes = BTreeMap::new();
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_none_or(|extension| extension != "lua") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("invalid recipe filename: {}", path.display()))?;
            let source =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            recipes.insert(name.to_string(), source);
        }
        let recipes: Vec<_> = recipes
            .iter()
            .map(|(name, source)| (name.as_str(), source.as_str()))
            .collect();
        Self::from_lua(&recipes)
    }

    fn from_lua(recipes: &[(&str, &str)]) -> Result<Self, String> {
        let mut packages = BTreeMap::new();
        for (name, source) in recipes {
            let lua = Lua::new();
            lua.set_memory_limit(4 * 1024 * 1024)
                .map_err(|e| e.to_string())?;
            let start = Instant::now();
            lua.set_interrupt(move |_| {
                if start.elapsed() > Duration::from_secs(1) {
                    return Err(mlua::Error::RuntimeError(
                        "package definition exceeded its execution limit".into(),
                    ));
                }
                Ok(mlua::VmState::Continue)
            });
            let environment = lua.create_table().map_err(|e| e.to_string())?;
            let value = lua
                .load(*source)
                .set_name(*name)
                .set_environment(environment)
                .eval()
                .map_err(|e| format!("{name}: {e}"))?;
            let package: CatalogPackage =
                lua.from_value(value).map_err(|e| format!("{name}: {e}"))?;
            if package.name != *name {
                return Err(format!("{name}: canonical name must match the filename"));
            }
            if packages.insert(name.to_string(), package).is_some() {
                return Err(format!("duplicate canonical package `{name}`"));
            }
        }
        let catalog = Self {
            schema: 1,
            packages,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    /// Checks identities, aliases, versions, backend requests, and smoke tests.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 || self.packages.is_empty() {
            return Err("catalog must use schema 1 and contain packages".into());
        }
        let mut names = BTreeSet::new();
        for (name, package) in &self.packages {
            if name != &package.name
                || package.description.trim().is_empty()
                || !package.homepage.starts_with("https://")
            {
                return Err(format!("{name}: invalid package identity"));
            }
            for identity in std::iter::once(name).chain(package.aliases.iter()) {
                if !valid_name(identity) || !names.insert(identity) {
                    return Err(format!("invalid or duplicate package name `{identity}`"));
                }
            }
            if !package.versions.contains_key(&package.default_version) {
                return Err(format!("{name}: default version has no recipe"));
            }
            for (version, recipe) in &package.versions {
                recipe
                    .validate()
                    .map_err(|e| format!("{name}@{version}: {e}"))?;
                if version.is_empty()
                    || version == "latest"
                    || version
                        .bytes()
                        .any(|c| c.is_ascii_whitespace() || matches!(c, b'/' | b':' | b'@'))
                {
                    return Err(format!("{name}: invalid version `{version}`"));
                }
            }
        }
        super::build::validate_dependencies(self)?;
        Ok(())
    }

    /// Finds a package by canonical name or an explicitly declared alias.
    pub fn find(&self, name: &str) -> Option<&CatalogPackage> {
        self.packages.get(name).or_else(|| {
            self.packages
                .values()
                .find(|package| package.aliases.iter().any(|alias| alias == name))
        })
    }

    /// Returns a stable digest of the evaluated, ordered catalog data.
    pub fn sha256(&self) -> String {
        hash_bytes(&serde_json::to_vec(self).expect("catalog serialization cannot fail"))
    }

    /// Exports the validated catalog as deterministic JSON for inspection or CI.
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
}

impl CatalogRecipe {
    fn validate(&self) -> Result<(), String> {
        if self.revision == 0 || self.source.is_some() == self.build.is_some() {
            return Err("recipe needs a revision and exactly one of source or build".into());
        }
        if let Some(build) = &self.build {
            build.validate()?;
        }
        if let Some(source) = &self.source {
            let request = PackageRequest::parse(source);
            if !matches!(request.resolver.as_deref(), Some("aqua" | "github"))
                || request
                    .version
                    .as_deref()
                    .is_none_or(|version| version == "latest")
                || !request
                    .name
                    .split_once('/')
                    .is_some_and(|(owner, repo)| !owner.is_empty() && !repo.is_empty())
            {
                return Err("recipe needs a revision and an exact aqua: or github: source".into());
            }
        }
        let systems: BTreeSet<_> = self.systems.iter().collect();
        if systems.is_empty()
            || systems.len() != self.systems.len()
            || systems.iter().any(|system| {
                !matches!(
                    system.as_str(),
                    "aarch64-macos" | "x86_64-macos" | "aarch64-linux" | "x86_64-linux"
                )
            })
        {
            return Err("recipe needs unique supported systems".into());
        }
        let bins: BTreeSet<_> = self.bins.iter().collect();
        if bins.is_empty()
            || bins.len() != self.bins.len()
            || bins.iter().any(|bin| !valid_name(bin))
        {
            return Err("recipe needs unique exported command names".into());
        }
        if self.checks.is_empty()
            || self
                .checks
                .iter()
                .any(|check| check.first().is_none_or(|bin| !bins.contains(bin)))
        {
            return Err("checks must execute declared commands".into());
        }
        Ok(())
    }
}

fn valid_name(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

pub(super) struct CatalogResolver {
    catalog: Option<PackageCatalog>,
    inputs: PackageResolverInputs,
    backends: ResolverStack,
}

impl CatalogResolver {
    pub(super) fn with_catalog(mut self, catalog: &PackageCatalog) -> Self {
        self.catalog = Some(catalog.clone());
        self
    }

    pub(super) fn new(inputs: &PackageResolverInputs, backends: ResolverStack) -> Self {
        Self {
            catalog: None,
            inputs: inputs.clone(),
            backends,
        }
    }
}

impl PackageResolver for CatalogResolver {
    fn name(&self) -> &str {
        "rootbeer"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        let catalog = match &self.catalog {
            Some(catalog) => catalog,
            None => PackageCatalog::embedded()?,
        };
        let Some(package) = catalog.find(&request.name) else {
            return Err(format!("unknown catalog package `{}`; use `rb package list` or an explicit backend request", request.name));
        };
        if request.asset.is_some() || !request.bins.is_empty() {
            return Err("catalog recipes own assets and commands; use an explicit backend request for overrides".into());
        }
        let digest = catalog.sha256();
        if self
            .inputs
            .catalog_sha256()
            .is_some_and(|expected| expected != digest)
        {
            return Err("the locked catalog differs from this binary; use the matching rb build or explicitly refresh with --update".into());
        }
        let version = request
            .version
            .as_deref()
            .unwrap_or(&package.default_version);
        let recipe = package.versions.get(version).ok_or_else(|| {
            format!(
                "{}@{version} is not in the catalog; available: {}",
                package.name,
                package
                    .versions
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        if !recipe.systems.contains(&context.system) {
            return Err(format!(
                "{}@{version} has no recipe for {}",
                package.name, context.system
            ));
        }
        let source = recipe.source.as_deref().ok_or_else(|| format!(
            "{}@{version} has a source recipe but no published binary; use `rb package build {} --output <directory>` explicitly",
            package.name, package.name
        ))?;
        let source = PackageRequest::parse(source);
        let resolution = self
            .backends
            .resolve_package(&source, context)
            .map_err(|e| e.to_string())?;
        let mut locked = resolution.package;
        for bin in &recipe.bins {
            if !locked.provides.bins.contains_key(bin) {
                return Err(format!(
                    "{}: backend did not provide declared command `{bin}`",
                    package.name
                ));
            }
        }
        locked
            .provides
            .bins
            .retain(|name, _| recipe.bins.contains(name));
        locked.name = package.name.clone();
        locked.version = version.to_string();
        Ok(Some(PackageResolution::new(
            locked,
            ResolutionProof::Catalog(CatalogProof {
                catalog_sha256: digest,
                name: package.name.clone(),
                version: version.to_string(),
                revision: recipe.revision,
                source,
                source_proof: Box::new(resolution.proof),
            }),
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::package::{
        GitReleaseProof, LockedInstall, LockedPackage, LockedSource, PackageRealizer, Provides,
        ResolverInput,
    };
    use crate::store::Store;

    #[test]
    fn embedded_catalog_has_canonical_names_and_stable_roundtrip() {
        let catalog = PackageCatalog::embedded().unwrap();
        assert_eq!(catalog.find("rg").unwrap().name, "ripgrep");
        assert!(catalog.find("BurntSushi/ripgrep").is_none());
        let decoded: PackageCatalog = serde_json::from_str(&catalog.to_json().unwrap()).unwrap();
        decoded.validate().unwrap();
        assert_eq!(catalog.sha256(), decoded.sha256());
    }

    #[test]
    fn default_stack_does_not_fall_back_for_unqualified_names() {
        let error = crate::package::default_resolver_stack()
            .resolve(
                &PackageRequest::parse("BurntSushi/ripgrep"),
                &ResolveContext::new("aarch64-macos"),
            )
            .unwrap_err();
        let crate::package::ResolveError::NotFound { attempts, .. } = error else {
            panic!("expected an unknown catalog name")
        };
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].resolver, "rootbeer");
        assert!(attempts[0].reason.contains("unknown catalog package"));
    }

    #[test]
    fn rejects_alias_collisions_and_invalid_recipes() {
        let original = PackageCatalog::embedded().unwrap();
        let mut catalog = original.clone();
        catalog
            .packages
            .get_mut("fd")
            .unwrap()
            .aliases
            .push("rg".into());
        assert!(catalog
            .validate()
            .unwrap_err()
            .contains("duplicate package name"));

        let mut catalog = original.clone();
        catalog
            .packages
            .get_mut("fd")
            .unwrap()
            .versions
            .get_mut("10.4.2")
            .unwrap()
            .source = Some("aqua:sharkdp/fd@latest".into());
        assert!(catalog.validate().unwrap_err().contains("exact"));

        let mut catalog = original.clone();
        catalog
            .packages
            .get_mut("fd")
            .unwrap()
            .versions
            .get_mut("10.4.2")
            .unwrap()
            .checks = vec![vec!["sh".into(), "-c".into(), "true".into()]];
        assert!(catalog
            .validate()
            .unwrap_err()
            .contains("declared commands"));
    }

    #[test]
    fn recipe_evaluation_has_no_host_access_and_rejects_unknown_fields() {
        assert!(PackageCatalog::from_lua(&[("bad", "return os.getenv('HOME')")]).is_err());
        assert!(PackageCatalog::from_lua(&[("bad", "return require('rootbeer')")]).is_err());
        let source = include_str!("../../../../packages/age.lua").replacen(
            "return {",
            "return { typo = true,",
            1,
        );
        assert!(PackageCatalog::from_lua(&[("age", &source)])
            .unwrap_err()
            .contains("unknown field"));
    }

    struct LocalBackend {
        source: PathBuf,
    }

    impl PackageResolver for LocalBackend {
        fn name(&self) -> &str {
            "aqua"
        }

        fn resolve(
            &self,
            request: &PackageRequest,
            _: &ResolveContext,
        ) -> Result<Option<PackageResolution>, String> {
            assert_eq!(request.name, "BurntSushi/ripgrep");
            assert_eq!(request.version.as_deref(), Some("15.2.0"));
            Ok(Some(PackageResolution::new(
                LockedPackage {
                    name: request.name.clone(),
                    version: "15.2.0".into(),
                    source: LockedSource::File {
                        path: self.source.clone(),
                        sha256: crate::store::hash_file(&self.source).unwrap(),
                    },
                    install: LockedInstall::Binary {
                        path: PathBuf::from("rg"),
                    },
                    provides: Provides {
                        bins: BTreeMap::from([("rg".into(), "rg".into())]),
                    },
                    output_sha256: None,
                },
                ResolutionProof::GitRelease(GitReleaseProof {
                    host: "github.com".into(),
                    owner: "BurntSushi".into(),
                    repo: "ripgrep".into(),
                    tag: Some("15.2.0".into()),
                    target_commit: None,
                    release_id: None,
                    documents: vec![],
                }),
            )))
        }
    }

    fn local_resolver(source: PathBuf) -> CatalogResolver {
        let mut backends = ResolverStack::new();
        backends.push(LocalBackend { source });
        CatalogResolver::new(&PackageResolverInputs::default(), backends)
    }

    #[test]
    fn canonical_and_alias_requests_share_identity_and_preserve_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("rg");
        std::fs::write(&source, "#!/bin/sh\nexit 0\n").unwrap();
        let resolver = local_resolver(source);
        let context = ResolveContext::new("aarch64-macos");
        let canonical = resolver
            .resolve(&PackageRequest::parse("ripgrep"), &context)
            .unwrap()
            .unwrap();
        let alias = resolver
            .resolve(&PackageRequest::parse("rg@15.2.0"), &context)
            .unwrap()
            .unwrap();
        assert_eq!(canonical, alias);
        assert_eq!(canonical.package.name, "ripgrep");
        let ResolutionProof::Catalog(proof) = &canonical.proof else {
            panic!("missing catalog provenance")
        };
        assert_eq!(proof.revision, 1);
        assert_eq!(
            proof.catalog_sha256,
            PackageCatalog::embedded().unwrap().sha256()
        );
        assert_eq!(proof.source.name, "BurntSushi/ripgrep");

        let realizer = PackageRealizer::with_dirs(
            Store::new(dir.path().join("store")),
            dir.path().join("downloads"),
            dir.path().join("tmp"),
        );
        let realized = realizer.realize(&canonical.package).unwrap();
        assert!(realized.bins["rg"].is_file());
    }

    #[test]
    fn resolves_new_catalog_names_without_rebuilding_the_binary() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("rg");
        std::fs::write(&source, "test executable").unwrap();
        let mut catalog = PackageCatalog::embedded().unwrap().clone();
        let mut package = catalog.packages.remove("ripgrep").unwrap();
        package.name = "new-search".into();
        package.aliases.clear();
        catalog.packages.insert(package.name.clone(), package);
        catalog.validate().unwrap();
        let resolver = local_resolver(source).with_catalog(&catalog);
        let resolution = resolver
            .resolve(
                &PackageRequest::parse("new-search"),
                &ResolveContext::new("aarch64-linux"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(resolution.package.name, "new-search");
        let ResolutionProof::Catalog(proof) = resolution.proof else {
            panic!("expected catalog proof")
        };
        assert_eq!(proof.catalog_sha256, catalog.sha256());
    }

    #[test]
    fn loads_directory_recipes_with_the_same_validation_and_digest() {
        let directory = tempfile::tempdir().unwrap();
        for (name, source) in EMBEDDED_RECIPES.iter().rev() {
            std::fs::write(directory.path().join(format!("{name}.lua")), source).unwrap();
        }
        std::fs::write(directory.path().join("README.md"), "ignored").unwrap();
        let catalog = PackageCatalog::from_directory(directory.path()).unwrap();
        assert_eq!(
            catalog.sha256(),
            PackageCatalog::embedded().unwrap().sha256()
        );
        std::fs::write(
            directory.path().join("invalid.lua"),
            "return os.execute('false')",
        )
        .unwrap();
        assert!(PackageCatalog::from_directory(directory.path()).is_err());
    }

    #[test]
    fn rejects_unknown_names_versions_platforms_overrides_and_catalog_changes_before_download() {
        let mut resolver = local_resolver(PathBuf::from("must-not-read"));
        let context = ResolveContext::new("aarch64-macos");
        for request in ["git", "ripgrep@0.0.0", "ripgrep@latest"] {
            assert!(resolver
                .resolve(&PackageRequest::parse(request), &context)
                .is_err());
        }
        assert!(resolver
            .resolve(
                &PackageRequest::parse("ripgrep"),
                &ResolveContext::new("x86_64-windows")
            )
            .unwrap_err()
            .contains("no recipe"));
        let mut request = PackageRequest::parse("ripgrep");
        request.asset = Some("different.tar.gz".into());
        assert!(resolver
            .resolve(&request, &context)
            .unwrap_err()
            .contains("overrides"));
        resolver.inputs.resolvers.insert(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: "different".into(),
            },
        );
        assert!(resolver
            .resolve(&PackageRequest::parse("ripgrep"), &context)
            .unwrap_err()
            .contains("--update"));
    }
}
