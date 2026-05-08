use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use super::{
    ArchiveFormat, LockedInstall, LockedPackage, LockedSource, PackageRequest, PackageResolver,
    Provides, ResolveContext,
};
use crate::store::hash_bytes;

#[derive(Debug, Clone)]
pub struct AquaResolver {
    registry_base_url: String,
}

impl AquaResolver {
    pub fn new() -> Self {
        Self {
            registry_base_url: "https://raw.githubusercontent.com/aquaproj/aqua-registry/main/pkgs"
                .to_string(),
        }
    }

    #[cfg(test)]
    fn with_registry_base_url(registry_base_url: impl Into<String>) -> Self {
        Self {
            registry_base_url: registry_base_url.into(),
        }
    }

    fn resolve_inner(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<LockedPackage>, String> {
        let (owner, repo) = request.name.split_once('/').ok_or_else(|| {
            "aqua packages must be requested as `owner/repo`, e.g. `aqua:FiloSottile/age`"
                .to_string()
        })?;

        let version = match &request.version {
            Some(version) => version.clone(),
            None => self.latest_version(owner, repo)?,
        };

        let registry = self.registry(owner, repo)?;
        let Some(package) = registry.packages.into_iter().find(|package| {
            matches!(
                package.package_type.as_deref(),
                Some("github_release" | "http")
            ) && package.repo_owner.as_deref().unwrap_or(owner) == owner
                && package.repo_name.as_deref().unwrap_or(repo) == repo
        }) else {
            return Ok(None);
        };

        let package = package.package_for(&version, context)?;
        package.validate_env(context)?;
        let format = package
            .format
            .clone()
            .unwrap_or_else(|| "tar.gz".to_string());

        if format != "tar.gz" && format != "tgz" {
            return Err(format!(
                "aqua package `{}` resolved to unsupported format `{format}`",
                request.name
            ));
        }

        let vars = TemplateVars::new(&version, context, &format, &package.replacements);
        let source_url = match package.package_type.as_deref() {
            Some("github_release") => {
                let asset = render(
                    package
                        .asset
                        .as_deref()
                        .ok_or_else(|| format!("aqua package `{}` has no asset", request.name))?,
                    &vars,
                );
                let owner = package.repo_owner.as_deref().unwrap_or(owner);
                let repo = package.repo_name.as_deref().unwrap_or(repo);
                format!("https://github.com/{owner}/{repo}/releases/download/{version}/{asset}")
            }

            Some("http") => render(
                package
                    .url
                    .as_deref()
                    .ok_or_else(|| format!("aqua package `{}` has no url", request.name))?,
                &vars,
            ),
            _ => return Ok(None),
        };

        let bytes = read_url(&source_url)?;
        let sha256 = hash_bytes(&bytes);
        let asset_without_ext = asset_without_ext(&source_url);
        let vars = vars.with_asset_without_ext(asset_without_ext);

        Ok(Some(LockedPackage {
            name: request.name.clone(),
            version,
            source: LockedSource::Url {
                url: source_url,
                sha256,
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: None,
            },
            provides: Provides {
                bins: package.provides(repo, &vars),
            },
            output_sha256: None,
        }))
    }

    fn latest_version(&self, owner: &str, repo: &str) -> Result<String, String> {
        let url = format!("{}/{owner}/{repo}/pkg.yaml", self.registry_base_url);
        let bytes = read_url(&url)?;
        let pkg: AquaPkg = serde_yml::from_slice(&bytes)
            .map_err(|e| format!("failed to parse aqua package index {url}: {e}"))?;

        let Some(package) = pkg.packages.first() else {
            return Err(format!("aqua package `{owner}/{repo}` has no versions"));
        };

        package.version(owner, repo).ok_or_else(|| {
            format!("aqua package `{owner}/{repo}` latest entry does not include a version")
        })
    }

    fn registry(&self, owner: &str, repo: &str) -> Result<AquaRegistry, String> {
        let url = format!("{}/{owner}/{repo}/registry.yaml", self.registry_base_url);
        let bytes = read_url(&url)?;
        serde_yml::from_slice(&bytes)
            .map_err(|e| format!("failed to parse aqua registry {url}: {e}"))
    }
}

impl Default for AquaResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl PackageResolver for AquaResolver {
    fn name(&self) -> &str {
        "aqua"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<LockedPackage>, String> {
        self.resolve_inner(request, context)
    }
}

#[derive(Debug, Deserialize)]
struct AquaPkg {
    #[serde(default)]
    packages: Vec<AquaPkgPackage>,
}

#[derive(Debug, Deserialize)]
struct AquaPkgPackage {
    name: String,
    version: Option<String>,
}

impl AquaPkgPackage {
    fn version(&self, owner: &str, repo: &str) -> Option<String> {
        self.version.clone().or_else(|| {
            self.name
                .strip_prefix(&format!("{owner}/{repo}@"))
                .map(str::to_string)
        })
    }
}

#[derive(Debug, Deserialize)]
struct AquaRegistry {
    #[serde(default)]
    packages: Vec<AquaPackage>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AquaPackage {
    #[serde(rename = "type")]
    package_type: Option<String>,
    repo_owner: Option<String>,
    repo_name: Option<String>,
    asset: Option<String>,
    url: Option<String>,
    format: Option<String>,
    #[serde(default)]
    files: Vec<AquaFile>,
    #[serde(default)]
    replacements: BTreeMap<String, String>,
    #[serde(default)]
    supported_envs: Vec<String>,
    #[serde(default)]
    version_overrides: Vec<AquaPackage>,
    version_constraint: Option<String>,
    no_asset: Option<bool>,
}

impl AquaPackage {
    fn package_for(&self, version: &str, context: &ResolveContext) -> Result<Self, String> {
        let mut package = self.clone();
        if let Some(override_package) = self
            .version_overrides
            .iter()
            .find(|candidate| candidate.matches_exact_version(version))
            .or_else(|| {
                self.version_overrides
                    .iter()
                    .find(|candidate| candidate.version_constraint.as_deref() == Some("true"))
            })
        {
            package.apply(override_package);
        }

        if package.no_asset.unwrap_or(false) {
            return Err(format!(
                "aqua package version `{version}` does not provide a downloadable asset"
            ));
        }

        if package.asset.is_none() && package.url.is_none() {
            return Err(format!(
                "aqua package version `{version}` has no asset for {}",
                context.system
            ));
        }

        Ok(package)
    }

    fn apply(&mut self, other: &AquaPackage) {
        self.package_type = other
            .package_type
            .clone()
            .or_else(|| self.package_type.clone());
        self.repo_owner = other.repo_owner.clone().or_else(|| self.repo_owner.clone());
        self.repo_name = other.repo_name.clone().or_else(|| self.repo_name.clone());
        self.asset = other.asset.clone().or_else(|| self.asset.clone());
        self.url = other.url.clone().or_else(|| self.url.clone());
        self.format = other.format.clone().or_else(|| self.format.clone());
        if !other.files.is_empty() {
            self.files = other.files.clone();
        }
        self.replacements.extend(other.replacements.clone());
        if !other.supported_envs.is_empty() {
            self.supported_envs = other.supported_envs.clone();
        }
        self.no_asset = other.no_asset.or(self.no_asset);
    }

    fn matches_exact_version(&self, version: &str) -> bool {
        let Some(constraint) = &self.version_constraint else {
            return false;
        };
        constraint == &format!("Version == \"{version}\"")
    }

    fn validate_env(&self, context: &ResolveContext) -> Result<(), String> {
        if self.supported_envs.is_empty() {
            return Ok(());
        }

        let Some((arch, os)) = context.system.split_once('-') else {
            return Ok(());
        };
        let aqua_os = aqua_os(os);
        let aqua_arch = aqua_arch(arch);
        if self.supported_envs.iter().any(|env| {
            env == "all"
                || env == aqua_os
                || env == aqua_arch
                || env == &format!("{aqua_os}/{aqua_arch}")
        }) {
            return Ok(());
        }

        Err(format!(
            "aqua package does not support {}; supported envs: {}",
            context.system,
            self.supported_envs.join(", ")
        ))
    }

    fn provides(&self, repo: &str, vars: &TemplateVars) -> BTreeMap<String, PathBuf> {
        if self.files.is_empty() {
            return BTreeMap::from([(repo.to_string(), PathBuf::from(repo))]);
        }

        self.files
            .iter()
            .map(|file| {
                let name = file.name.clone();
                let src = file.src.as_deref().unwrap_or(&file.name);
                (name, PathBuf::from(render(src, vars)))
            })
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AquaFile {
    name: String,
    src: Option<String>,
}

#[derive(Debug)]
struct TemplateVars {
    version: String,
    os: String,
    arch: String,
    format: String,
    asset_without_ext: String,
}

impl TemplateVars {
    fn new(
        version: &str,
        context: &ResolveContext,
        format: &str,
        replacements: &BTreeMap<String, String>,
    ) -> Self {
        let (arch, os) = context.system.split_once('-').unwrap_or(("amd64", "linux"));
        let mut os = aqua_os(os).to_string();
        let mut arch = aqua_arch(arch).to_string();

        if let Some(replacement) = replacements.get(&os) {
            os = replacement.clone();
        }
        if let Some(replacement) = replacements.get(&arch) {
            arch = replacement.clone();
        }

        Self {
            version: version.to_string(),
            os,
            arch,
            format: format.to_string(),
            asset_without_ext: String::new(),
        }
    }

    fn with_asset_without_ext(mut self, asset_without_ext: String) -> Self {
        self.asset_without_ext = asset_without_ext;
        self
    }
}

fn render(template: &str, vars: &TemplateVars) -> String {
    template
        .replace("{{.Version}}", &vars.version)
        .replace("{{trimV .Version}}", vars.version.trim_start_matches('v'))
        .replace("{{.OS}}", &vars.os)
        .replace("{{.Arch}}", &vars.arch)
        .replace("{{.Format}}", &vars.format)
        .replace("{{.AssetWithoutExt}}", &vars.asset_without_ext)
}

fn aqua_os(os: &str) -> &str {
    match os {
        "macos" => "darwin",
        other => other,
    }
}

fn aqua_arch(arch: &str) -> &str {
    match arch {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    }
}

fn asset_without_ext(url: &str) -> String {
    let asset = url.rsplit('/').next().unwrap_or(url);
    asset
        .strip_suffix(".tar.gz")
        .or_else(|| asset.strip_suffix(".tgz"))
        .or_else(|| asset.strip_suffix(".zip"))
        .unwrap_or(asset)
        .to_string()
}

fn read_url(url: &str) -> Result<Vec<u8>, String> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| format!("failed to read {url}: {e}"));
    }

    ureq::get(url)
        .call()
        .map_err(|e| format!("failed to fetch {url}: {e}"))?
        .into_body()
        .read_to_vec()
        .map_err(|e| format!("failed to read {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn resolves_simple_http_aqua_package() {
        let tmp = tempfile::tempdir().unwrap();
        let package_dir = tmp.path().join("owner/tool");
        fs::create_dir_all(&package_dir).unwrap();
        let archive = tmp.path().join("tool-v1.0.0-darwin-arm64.tar.gz");
        fs::write(&archive, b"archive bytes").unwrap();
        fs::write(
            package_dir.join("pkg.yaml"),
            "packages:\n  - name: owner/tool@v1.0.0\n",
        )
        .unwrap();
        fs::write(
            package_dir.join("registry.yaml"),
            format!(
                r#"
packages:
  - type: http
    repo_owner: owner
    repo_name: tool
    url: file://{}/tool-{{{{.Version}}}}-{{{{.OS}}}}-{{{{.Arch}}}}.{{{{.Format}}}}
    format: tar.gz
    files:
      - name: tool
        src: tool/bin/tool
    supported_envs:
      - darwin/arm64
"#,
                tmp.path().display()
            ),
        )
        .unwrap();

        let resolver =
            AquaResolver::with_registry_base_url(format!("file://{}", tmp.path().display()));
        let package = resolver
            .resolve(
                &PackageRequest::new("owner/tool"),
                &ResolveContext::new("aarch64-macos"),
            )
            .unwrap()
            .unwrap();

        assert_eq!(package.name, "owner/tool");
        assert_eq!(package.version, "v1.0.0");
        assert_eq!(
            package.source,
            LockedSource::Url {
                url: format!(
                    "file://{}/tool-v1.0.0-darwin-arm64.tar.gz",
                    tmp.path().display()
                ),
                sha256: hash_bytes(b"archive bytes"),
            }
        );
        assert_eq!(
            package.provides.bins.get("tool"),
            Some(&PathBuf::from("tool/bin/tool"))
        );
    }

    #[test]
    fn renders_replacements_and_asset_without_ext() {
        let vars = TemplateVars::new(
            "v1.2.3",
            &ResolveContext::new("x86_64-macos"),
            "tar.gz",
            &BTreeMap::from([("darwin".to_string(), "apple-darwin".to_string())]),
        )
        .with_asset_without_ext("tool-v1.2.3".to_string());

        assert_eq!(
            render(
                "tool_{{trimV .Version}}_{{.OS}}_{{.Arch}}/{{.AssetWithoutExt}}",
                &vars
            ),
            "tool_1.2.3_apple-darwin_amd64/tool-v1.2.3"
        );
    }
}
