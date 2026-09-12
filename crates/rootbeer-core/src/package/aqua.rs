use std::collections::BTreeMap;
use std::path::PathBuf;

use aqua_registry::{AquaPackage, AquaPackageType};
use serde::Deserialize;

use super::download::{read_url, DownloadCache};
use super::{
    ArchiveFormat, GitHubRepositoryPin, LockedInstall, LockedPackage, LockedSource,
    MetadataDocumentProof, PackageRequest, PackageResolution, PackageResolver, Provides,
    ResolutionProof, ResolveContext, SnapshotProof, SnapshotSource,
};
use crate::store::hash_bytes;

#[derive(Debug, Clone)]
pub struct AquaResolver {
    registry_base_url: String,
    registry_source: SnapshotSource,
    downloads: DownloadCache,
}

impl AquaResolver {
    pub fn new() -> Self {
        Self {
            registry_base_url: "https://raw.githubusercontent.com/aquaproj/aqua-registry/main/pkgs"
                .to_string(),
            registry_source: SnapshotSource::Url {
                url: "https://raw.githubusercontent.com/aquaproj/aqua-registry/main/pkgs"
                    .to_string(),
            },
            downloads: DownloadCache::default(),
        }
    }

    pub fn from_registry_pin(pin: &GitHubRepositoryPin) -> Self {
        let registry_base_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/pkgs",
            pin.owner, pin.repo, pin.rev
        );
        Self {
            registry_base_url,
            registry_source: SnapshotSource::GitHubRepository(pin.clone()),
            downloads: DownloadCache::default(),
        }
    }

    #[cfg(test)]
    fn with_registry_base_url(
        registry_base_url: impl Into<String>,
        downloads: impl Into<PathBuf>,
    ) -> Self {
        let registry_base_url = registry_base_url.into();
        Self {
            registry_source: SnapshotSource::Url {
                url: registry_base_url.clone(),
            },
            registry_base_url,
            downloads: DownloadCache::new(downloads),
        }
    }

    fn resolve_inner(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        let (owner, repo) = request.name.split_once('/').ok_or_else(|| {
            "aqua packages must be requested as `owner/repo`, e.g. `aqua:FiloSottile/age`"
                .to_string()
        })?;

        let mut documents = Vec::new();
        let version = match &request.version {
            Some(version) if version != "latest" => version.clone(),
            _ => {
                let (version, document) = self.latest_version(owner, repo)?;
                documents.push(document);
                version
            }
        };

        let (registry, document) = self.registry(owner, repo)?;
        documents.push(document);
        let Some(package) = registry.packages.into_iter().find(|package| {
            package.name.as_deref() == Some(request.name.as_str())
                || (package.repo_owner == owner && package.repo_name == repo)
        }) else {
            return Ok(None);
        };
        let (arch, os) = context
            .system
            .split_once('-')
            .ok_or_else(|| format!("invalid target system `{}`", context.system))?;
        let os = aqua_os(os);
        let arch = aqua_arch(arch);
        if !package.version_constraint_ok(&[&version]) {
            return Err(format!(
                "aqua package `{}` does not support version `{version}`",
                request.name
            ));
        }
        let package = package.with_version(&[&version], os, arch);
        validate_package(&package, os, arch)?;
        let format = package
            .format(&version, os, arch)
            .map_err(|e| e.to_string())?;
        let source_url = match package.package_type() {
            AquaPackageType::GithubRelease => {
                let asset = package
                    .asset(&version, os, arch)
                    .map_err(|e| e.to_string())?;
                format!(
                    "https://github.com/{}/{}/releases/download/{version}/{asset}",
                    package.repo_owner, package.repo_name
                )
            }
            AquaPackageType::Http => package.url(&version, os, arch).map_err(|e| e.to_string())?,
            other => return Err(format!("unsupported aqua package type `{other}`")),
        };
        let bins = package_bins(&package, repo, &version, os, arch)?;
        let install = match format {
            "raw" => {
                if bins.len() != 1 {
                    return Err("raw aqua packages must provide exactly one binary".to_string());
                }
                LockedInstall::Binary {
                    path: bins.values().next().unwrap().clone(),
                }
            }
            "tar.gz" | "tgz" => LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: None,
            },
            "tar.xz" | "txz" => LockedInstall::Archive {
                format: ArchiveFormat::TarXz,
                strip_prefix: None,
            },
            "zip" => LockedInstall::Archive {
                format: ArchiveFormat::Zip,
                strip_prefix: None,
            },
            other => return Err(format!("unsupported aqua archive format `{other}`")),
        };

        let source = self
            .downloads
            .materialize(&source_url, None)
            .map_err(|e| format!("failed to fetch {source_url}: {e}"))?;

        let package = LockedPackage {
            name: request.name.clone(),
            version,
            source: LockedSource::Url {
                url: source_url,
                sha256: source.sha256,
            },
            install,
            provides: Provides { bins },
            output_sha256: None,
        };
        let proof = ResolutionProof::Snapshot(SnapshotProof {
            resolver: "aqua".to_string(),
            source: self.registry_source.clone(),
            documents,
        });

        Ok(Some(PackageResolution::new(package, proof)))
    }

    fn latest_version(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<(String, MetadataDocumentProof), String> {
        let url = format!("{}/{owner}/{repo}/pkg.yaml", self.registry_base_url);
        let bytes = read_url(&url).map_err(|e| e.to_string())?;
        let proof = MetadataDocumentProof {
            url: url.clone(),
            sha256: hash_bytes(&bytes),
        };
        let pkg: AquaPkg = serde_yml::from_slice(&bytes)
            .map_err(|e| format!("failed to parse aqua package index {url}: {e}"))?;

        let Some(package) = pkg.packages.first() else {
            return Err(format!("aqua package `{owner}/{repo}` has no versions"));
        };

        package
            .version(owner, repo)
            .map(|version| (version, proof))
            .ok_or_else(|| {
                format!("aqua package `{owner}/{repo}` latest entry does not include a version")
            })
    }

    fn registry(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<(AquaRegistry, MetadataDocumentProof), String> {
        let url = format!("{}/{owner}/{repo}/registry.yaml", self.registry_base_url);
        let bytes = read_url(&url).map_err(|e| e.to_string())?;
        let proof = MetadataDocumentProof {
            url: url.clone(),
            sha256: hash_bytes(&bytes),
        };
        serde_yml::from_slice(&bytes)
            .map(|registry| (registry, proof))
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
    ) -> Result<Option<PackageResolution>, String> {
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

fn validate_package(package: &AquaPackage, os: &str, arch: &str) -> Result<(), String> {
    if package.no_asset.unwrap_or(false) {
        return Err("aqua package version has no downloadable asset".to_string());
    }
    if let Some(message) = &package.error_message {
        return Err(message.clone());
    }
    if package.rosetta2.unwrap_or(false) && os == "darwin" && arch == "arm64" {
        return Err("aqua package requires Rosetta; native builds only are supported".to_string());
    }
    if !package.supported_envs.is_empty()
        && !package
            .supported_envs
            .iter()
            .any(|env| env == "all" || env == os || env == arch || env == &format!("{os}/{arch}"))
    {
        return Err(format!("aqua package does not support {os}/{arch}"));
    }
    Ok(())
}

fn package_bins(
    package: &AquaPackage,
    repo: &str,
    version: &str,
    os: &str,
    arch: &str,
) -> Result<BTreeMap<String, PathBuf>, String> {
    if package.files.is_empty() {
        return Ok(BTreeMap::from([(repo.to_string(), PathBuf::from(repo))]));
    }
    package
        .files
        .iter()
        .map(|file| {
            let src = file
                .src(package, version, os, arch)
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| file.name.clone());
            Ok((file.name.clone(), PathBuf::from(src)))
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::store::hash_bytes;

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

        let resolver = AquaResolver::with_registry_base_url(
            format!("file://{}", tmp.path().display()),
            tmp.path().join("downloads"),
        );
        let resolution = resolver
            .resolve(
                &PackageRequest::new("owner/tool"),
                &ResolveContext::new("aarch64-macos"),
            )
            .unwrap()
            .unwrap();
        let package = resolution.package;

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
        let ResolutionProof::Snapshot(proof) = resolution.proof else {
            panic!("expected aqua snapshot proof");
        };
        assert_eq!(proof.resolver, "aqua");
        assert_eq!(proof.documents.len(), 2);
        let latest = resolver
            .resolve(
                &PackageRequest::new("owner/tool").version("latest"),
                &ResolveContext::new("aarch64-macos"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(latest.package.version, "v1.0.0");
    }

    #[test]
    fn applies_semver_and_platform_overrides() {
        let package: AquaPackage = serde_yml::from_str(
            r#"
repo_owner: neovim
repo_name: neovim
version_constraint: "false"
version_overrides:
  - version_constraint: semver(">= 0.10.0")
    asset: nvim-{{.OS}}-{{.Arch}}.tar.gz
    replacements:
      darwin: macos
    files:
      - name: nvim
        src: "{{.AssetWithoutExt}}/bin/nvim"
    overrides:
      - goos: darwin
        replacements:
          arm64: arm64
"#,
        )
        .unwrap();
        assert!(package.version_constraint_ok(&["v0.11.0"]));
        assert!(!package.version_constraint_ok(&["v0.1.0"]));
        let package = package.with_version(&["v0.11.0"], "darwin", "arm64");
        assert_eq!(
            package.asset("v0.11.0", "darwin", "arm64").unwrap(),
            "nvim-macos-arm64.tar.gz"
        );
        assert_eq!(
            package_bins(&package, "neovim", "v0.11.0", "darwin", "arm64").unwrap()["nvim"],
            PathBuf::from("nvim-macos-arm64/bin/nvim")
        );
    }
}
