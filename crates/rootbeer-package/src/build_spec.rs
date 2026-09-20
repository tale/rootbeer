use crate::{ArchiveFormat, LockedPackage, PackageRequest, PackageResolverInputs};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, PathBuf};

/// Supported source compilation mechanisms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildBackend {
    Autotools,
    Custom,
    Zig,
    Rust,
    Go,
}

/// Which exports a dependency contributes to its consumer's build environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    All,
    Build,
    Link,
    Runtime,
    LinkRuntime,
}

impl DependencyKind {
    pub fn is_runtime(self) -> bool {
        matches!(self, Self::Runtime | Self::LinkRuntime)
    }
}

pub fn is_library_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".a")
                || name.ends_with(".dylib")
                || name.ends_with(".so")
                || name.contains(".so.")
        })
}

/// Exact package input; strings retain the original combined export behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum BuildDependency {
    All(String),
    Scoped {
        package: String,
        kind: DependencyKind,
    },
}

impl BuildDependency {
    pub fn package(&self) -> &str {
        match self {
            Self::All(package) | Self::Scoped { package, .. } => package,
        }
    }

    pub fn kind(&self) -> DependencyKind {
        match self {
            Self::All(_) => DependencyKind::All,
            Self::Scoped { kind, .. } => *kind,
        }
    }
}

impl From<String> for BuildDependency {
    fn from(package: String) -> Self {
        Self::All(package)
    }
}

impl From<&str> for BuildDependency {
    fn from(package: &str) -> Self {
        Self::All(package.into())
    }
}

/// Verified source inputs and exact canonical build dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBuild {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<crate::GitSource>,
    pub backend: BuildBackend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust: Option<RustBuild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go: Option<GoBuild>,
    pub url: String,
    pub sha256: String,
    #[serde(
        serialize_with = "serialize_archive",
        deserialize_with = "deserialize_archive"
    )]
    pub archive: ArchiveFormat,
    pub strip_prefix: PathBuf,
    #[serde(default)]
    pub configure: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patches: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<BuildDependency>,
    /// Static or shared libraries exported for other source packages to link against.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<BuildSteps>,
}

/// Go binary entry points and compile-time inputs within a locked module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoBuild {
    pub binaries: BTreeMap<String, String>,
    #[serde(default)]
    pub generate: Vec<String>,
    #[serde(default)]
    pub experiments: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default, rename = "cgo")]
    pub is_cgo_enabled: bool,
}

impl GoBuild {
    fn validate(&self) -> Result<(), String> {
        let is_package = |package: &str| {
            package == "."
                || package.strip_prefix("./").is_some_and(|path| {
                    !path.is_empty()
                        && path.split('/').all(|part| {
                            !part.is_empty()
                                && part != "."
                                && part != ".."
                                && part.bytes().all(|byte| {
                                    byte.is_ascii_alphanumeric() || b"_-".contains(&byte)
                                })
                        })
                })
        };
        if self.binaries.is_empty() {
            return Err("Go builds require explicit binary entry points".into());
        }
        for (name, package) in &self.binaries {
            if name.is_empty()
                || name == "."
                || name == ".."
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-+.".contains(&byte))
                || !is_package(package)
            {
                return Err("Go binaries require simple names and local package paths".into());
            }
        }
        if self.generate.iter().any(|package| !is_package(package)) {
            return Err("Go generators require explicit local package paths".into());
        }
        if self.tags.iter().chain(&self.experiments).any(|tag| {
            tag.is_empty()
                || !tag
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.".contains(&byte))
        }) {
            return Err("invalid Go build tag or experiment".into());
        }
        for (name, value) in &self.variables {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_./-".contains(&byte))
                || value
                    .chars()
                    .any(|ch| ch.is_control() || matches!(ch, '\'' | '"'))
            {
                return Err("invalid Go linker variable".into());
            }
        }
        Ok(())
    }
}

/// Cargo workspace selection and compile-time inputs for Rust binaries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustBuild {
    pub packages: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub no_default_features: bool,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

impl RustBuild {
    fn validate(&self) -> Result<(), String> {
        let valid = |value: &str| {
            !value.is_empty()
                && !value.starts_with('-')
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-/".contains(&byte))
        };
        if self.packages.is_empty()
            || self
                .packages
                .iter()
                .any(|name| !valid(name) || name.contains('/'))
            || self.features.iter().any(|name| !valid(name))
        {
            return Err(
                "Rust builds require explicit Cargo packages and valid feature names".into(),
            );
        }
        for (name, value) in &self.environment {
            if name.is_empty()
                || name.starts_with(|ch: char| ch.is_ascii_digit())
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
                || ["CARGO", "RUST", "LD_", "DYLD_"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
                || matches!(
                    name.as_str(),
                    "PATH"
                        | "HOME"
                        | "TMPDIR"
                        | "CC"
                        | "CXX"
                        | "AR"
                        | "RANLIB"
                        | "MAKEFLAGS"
                        | "SOURCE_DATE_EPOCH"
                        | "ZERO_AR_DATE"
                        | "CONFIG_SHELL"
                        | "LC_ALL"
                        | "TZ"
                )
                || value.contains('\0')
            {
                return Err(format!("Rust build cannot set environment variable {name}"));
            }
        }
        Ok(())
    }
}

/// Explicit command phases for source projects without a built-in preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSteps {
    pub configure: Vec<Vec<String>>,
    pub build: Vec<Vec<String>>,
    pub check: Vec<Vec<String>>,
    pub install: Vec<Vec<String>>,
}

fn serialize_archive<S: serde::Serializer>(
    format: &ArchiveFormat,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(match format {
        ArchiveFormat::TarGz => "tar.gz",
        ArchiveFormat::TarXz => "tar.xz",
        ArchiveFormat::Zip => "zip",
    })
}

fn deserialize_archive<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArchiveFormat, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "tar.gz" => Ok(ArchiveFormat::TarGz),
        "tar.xz" => Ok(ArchiveFormat::TarXz),
        "zip" => Ok(ArchiveFormat::Zip),
        other => Err(serde::de::Error::custom(format!(
            "unsupported source archive `{other}`"
        ))),
    }
}

impl SourceBuild {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(git) = &self.git {
            git.validate()?;
        }
        if !self.url.starts_with("https://")
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err("source builds require an HTTPS URL and lowercase SHA-256".into());
        }
        if self.strip_prefix.as_os_str().is_empty()
            || self
                .strip_prefix
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err("source strip_prefix must remain within the archive".into());
        }
        if self
            .configure
            .iter()
            .any(|arg| arg.starts_with("--prefix") || !arg.starts_with("--"))
        {
            return Err("configure options must be flags; the builder owns --prefix".into());
        }
        match self.backend {
            BuildBackend::Autotools if !self.args.is_empty() => {
                return Err("Autotools builds use configure, not args".into());
            }
            BuildBackend::Zig if !self.configure.is_empty() => {
                return Err("Zig builds use args, not configure".into());
            }
            BuildBackend::Custom if !self.configure.is_empty() || !self.args.is_empty() => {
                return Err("custom builds use steps, not configure or args".into());
            }
            BuildBackend::Zig
                if self
                    .args
                    .iter()
                    .any(|arg| !arg.starts_with("-D") || arg.len() == 2) =>
            {
                return Err(
                    "Zig args must be -D options; the builder owns prefix and caches".into(),
                );
            }
            _ => {}
        }
        if matches!(self.backend, BuildBackend::Rust) != self.rust.is_some() {
            return Err("rust settings are required only for the Rust backend".into());
        }
        if let Some(rust) = &self.rust {
            rust.validate()?;
            if !self.configure.is_empty() || !self.args.is_empty() || !self.libraries.is_empty() {
                return Err("Rust builds use rust settings and export binaries".into());
            }
        }
        if matches!(self.backend, BuildBackend::Go) != self.go.is_some() {
            return Err("go settings are required only for the Go backend".into());
        }
        if let Some(go) = &self.go {
            go.validate()?;
            if !self.configure.is_empty() || !self.args.is_empty() || !self.libraries.is_empty() {
                return Err("Go builds use go settings and export binaries".into());
            }
        }
        if matches!(self.backend, BuildBackend::Custom) != self.steps.is_some() {
            return Err("steps are required only for the custom backend".into());
        }
        if let Some(steps) = &self.steps {
            if steps.build.is_empty() || steps.check.is_empty() || steps.install.is_empty() {
                return Err("custom builds require build, check, and install commands".into());
            }
            for command in steps
                .configure
                .iter()
                .chain(&steps.build)
                .chain(&steps.check)
                .chain(&steps.install)
            {
                if command
                    .first()
                    .is_none_or(|program| program.trim().is_empty())
                    || command.iter().any(|argument| argument.contains('\0'))
                {
                    return Err(
                        "build steps require nonempty command arrays without NUL bytes".into(),
                    );
                }
            }
        }
        if self.patches.iter().any(|patch| patch.trim().is_empty()) {
            return Err("source patches must not be empty".into());
        }
        let mut libraries = BTreeSet::new();
        for path in &self.libraries {
            crate::realize::validate_relative_path("library", path).map_err(|e| e.to_string())?;
            if !matches!(path.components().next(), Some(Component::Normal(name)) if name == "lib" || name == "lib64")
                || !is_library_path(path)
                || !libraries.insert(path)
            {
                return Err(
                    "libraries must name unique static or shared libraries under lib or lib64"
                        .into(),
                );
            }
        }
        let mut dependencies = BTreeSet::new();
        for dependency in &self.dependencies {
            let request = PackageRequest::parse(dependency.package());
            if request.resolver.is_some()
                || request.version.is_none()
                || !dependencies.insert(dependency.package())
            {
                return Err("build dependencies require canonical name@version requests".into());
            }
        }
        Ok(())
    }
}

/// The build inputs, dependency outputs, host toolchain, and resulting artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifact {
    pub schema: u32,
    pub recipe_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<BuildEnvironmentLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_audit_sha256: Option<String>,
    pub catalog_sha256: String,
    pub revision: u32,
    pub system: String,
    pub build: SourceBuild,
    pub dependencies: BTreeMap<String, LockedPackage>,
    pub resolver_inputs: PackageResolverInputs,
    pub toolchain: BTreeMap<String, String>,
    pub package: LockedPackage,
}

/// Content identities and variables used by the build executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildEnvironmentLock {
    pub schema: u32,
    pub system: String,
    pub tools: BTreeMap<String, BuildEnvironmentInput>,
    pub inputs: BTreeMap<String, BuildEnvironmentInput>,
    pub variables: BTreeMap<String, String>,
}

/// A tool executable or a directory containing SDK or toolchain inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildEnvironmentInput {
    pub path: PathBuf,
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_is_the_only_explicit_phase_backend_name() {
        let backend: BuildBackend = serde_json::from_value(serde_json::json!("custom")).unwrap();
        assert!(matches!(backend, BuildBackend::Custom));
        assert_eq!(serde_json::to_value(backend).unwrap(), "custom");
        assert!(serde_json::from_value::<BuildBackend>(serde_json::json!("commands")).is_err());
    }
    #[test]
    fn dependency_roles_preserve_legacy_string_encoding() {
        let legacy: BuildDependency = serde_json::from_value(serde_json::json!("tool@1")).unwrap();
        assert_eq!(legacy.kind(), DependencyKind::All);
        assert_eq!(serde_json::to_value(legacy).unwrap(), "tool@1");
        let scoped = serde_json::json!({"package": "tool@1", "kind": "build"});
        let dependency: BuildDependency = serde_json::from_value(scoped.clone()).unwrap();
        assert_eq!(dependency.kind(), DependencyKind::Build);
        assert_eq!(serde_json::to_value(dependency).unwrap(), scoped);
        assert!(serde_json::from_value::<BuildDependency>(
            serde_json::json!({"package": "tool@1", "kind": "unknown"})
        )
        .is_err());
    }
}
