use crate::{ArchiveFormat, LockedPackage, PackageRequest, PackageResolverInputs};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, PathBuf};

/// Supported source compilation mechanisms.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildBackend {
    Autotools,
    #[serde(rename = "commands", alias = "custom")]
    Custom,
    Zig,
}

/// Which exports a dependency contributes to its consumer's build environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    All,
    Build,
    Link,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBuild {
    pub backend: BuildBackend,
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
    /// Static archives exported for other source packages to link against.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<BuildSteps>,
}

/// Explicit command phases for source projects without a built-in preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            crate::realize::validate_relative_path("static library", path)
                .map_err(|e| e.to_string())?;
            if !matches!(path.components().next(), Some(Component::Normal(name)) if name == "lib" || name == "lib64")
                || path.extension().is_none_or(|extension| extension != "a")
                || !libraries.insert(path)
            {
                return Err("libraries must name unique static archives under lib or lib64".into());
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<BuildEnvironmentLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<String>,
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
    fn custom_accepts_both_names_without_changing_existing_catalog_digests() {
        for name in ["commands", "custom"] {
            let backend: BuildBackend = serde_json::from_value(serde_json::json!(name)).unwrap();
            assert!(matches!(backend, BuildBackend::Custom));
            assert_eq!(serde_json::to_value(backend).unwrap(), "commands");
        }
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
            serde_json::json!({"package": "tool@1", "kind": "runtime"})
        )
        .is_err());
    }
}
