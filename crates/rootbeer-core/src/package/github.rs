use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::download::{read_url, DownloadCache};
use super::realize::{extract_archive, validate_relative_path};
use super::{
    ArchiveFormat, GitReleaseProof, LockedInstall, LockedPackage, LockedSource,
    MetadataDocumentProof, PackageRequest, PackageResolution, PackageResolver, Provides,
    ResolutionProof, ResolveContext,
};
use crate::store::hash_bytes;

#[derive(Debug, Clone)]
pub struct GitHubResolver {
    api_url: String,
    downloads: DownloadCache,
}

impl GitHubResolver {
    pub fn new() -> Self {
        Self {
            api_url: "https://api.github.com".to_string(),
            downloads: DownloadCache::default(),
        }
    }

    fn resolve_inner(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<PackageResolution, String> {
        let (owner, repo) = repository(&request.name)?;
        let endpoint = match request.version.as_deref() {
            None | Some("latest") => "latest".to_string(),
            Some(tag) => format!("tags/{}", encode_segment(tag)),
        };
        let url = format!("{}/repos/{owner}/{repo}/releases/{endpoint}", self.api_url);
        let bytes = read_url(&url).map_err(|e| e.to_string())?;
        let release: Release = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if release.draft || release.tag_name.is_empty() {
            return Err("GitHub release must have a tag and be published".to_string());
        }
        let asset = select_asset(&release.assets, request.asset.as_deref(), context)?;
        let expected = match asset.digest.as_deref() {
            Some(digest) if digest.starts_with("sha256:") => Some(&digest[7..]),
            _ => None,
        };
        let source = self
            .downloads
            .materialize(&asset.browser_download_url, expected)
            .map_err(|e| e.to_string())?;
        let (install, bins) = match archive_format(&asset.name) {
            Some(format) => {
                let bins = if request.bins.is_empty() {
                    let extracted = tempfile::tempdir().map_err(|e| e.to_string())?;
                    extract_archive(&source.path, format, extracted.path())
                        .map_err(|e| e.to_string())?;
                    discover_bins(extracted.path())?
                } else {
                    request.bins.clone()
                };
                (
                    LockedInstall::Archive {
                        format,
                        strip_prefix: None,
                    },
                    bins,
                )
            }
            None => {
                let bins = if request.bins.is_empty() {
                    BTreeMap::from([(repo.to_string(), PathBuf::from(repo))])
                } else {
                    request.bins.clone()
                };
                if bins.len() != 1 {
                    return Err("raw GitHub assets must provide exactly one binary".to_string());
                }
                let path = bins.values().next().unwrap().clone();
                validate_relative_path("binary path", &path).map_err(|e| e.to_string())?;
                (LockedInstall::Binary { path }, bins)
            }
        };
        let package = LockedPackage {
            name: request.name.clone(),
            version: release.tag_name.clone(),
            source: LockedSource::Url {
                url: asset.browser_download_url.clone(),
                sha256: source.sha256,
            },
            install,
            provides: Provides { bins },
            output_sha256: None,
        };
        let proof = ResolutionProof::GitRelease(GitReleaseProof {
            host: "github.com".to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            tag: Some(release.tag_name),
            target_commit: None,
            release_id: Some(release.id),
            documents: vec![MetadataDocumentProof {
                url,
                sha256: hash_bytes(&bytes),
            }],
        });
        Ok(PackageResolution::new(package, proof))
    }
}

impl Default for GitHubResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl PackageResolver for GitHubResolver {
    fn name(&self) -> &str {
        "github"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        self.resolve_inner(request, context).map(Some)
    }
}

#[derive(Debug, Deserialize)]
struct Release {
    id: u64,
    tag_name: String,
    #[serde(default)]
    draft: bool,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

fn repository(name: &str) -> Result<(&str, &str), String> {
    let Some((owner, repo)) = name.split_once('/') else {
        return Err("GitHub packages require `github:owner/repo@tag`".to_string());
    };
    if [owner, repo].iter().any(|part| {
        part.is_empty()
            || *part == "."
            || *part == ".."
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    }) {
        return Err(format!("invalid GitHub repository `{name}`"));
    }
    Ok((owner, repo))
}

fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn archive_format(name: &str) -> Option<ArchiveFormat> {
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some(ArchiveFormat::TarGz);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return Some(ArchiveFormat::TarXz);
    }
    name.ends_with(".zip").then_some(ArchiveFormat::Zip)
}

fn is_installable(name: &str) -> bool {
    if archive_format(name).is_some() {
        return !name.contains("source") && !name.contains("debug") && !name.contains("symbols");
    }
    let extension = name.rsplit('.').next().unwrap_or(name);
    ![
        "gz", "xz", "bz2", "zst", "7z", "tar", "deb", "rpm", "dmg", "pkg", "exe", "msi", "sha256",
        "sha512", "sig", "asc", "minisig", "sigstore", "txt", "json", "yml", "yaml", "pem", "spdx",
        "sbom",
    ]
    .contains(&extension)
        && !name.contains("checksum")
        && !name.contains("sha256sum")
        && !name.contains("sha512sum")
        && !name.contains("attestation")
}

fn select_asset<'a>(
    assets: &'a [Asset],
    selected: Option<&str>,
    context: &ResolveContext,
) -> Result<&'a Asset, String> {
    if let Some(name) = selected {
        let asset = assets
            .iter()
            .find(|asset| asset.name == name)
            .ok_or_else(|| format!("GitHub release has no asset named `{name}`"))?;
        if !is_installable(&asset.name.to_ascii_lowercase()) {
            return Err(format!("unsupported GitHub asset format `{name}`"));
        }
        return Ok(asset);
    }
    let (arch, os) = context
        .system
        .split_once('-')
        .ok_or_else(|| format!("invalid target system `{}`", context.system))?;
    let os_names: &[&str] = match os {
        "macos" => &["darwin", "macos", "osx"],
        "linux" => &["linux"],
        _ => return Err(format!("unsupported GitHub target OS `{os}`")),
    };
    let arch_names: &[&str] = match arch {
        "aarch64" => &["aarch64", "arm64"],
        "x86_64" => &["x86_64", "amd64", "x64"],
        _ => return Err(format!("unsupported GitHub target architecture `{arch}`")),
    };
    let mut candidates: Vec<&Asset> = assets
        .iter()
        .filter(|asset| {
            let name = asset.name.to_ascii_lowercase();
            is_installable(&name)
                && os_names.iter().any(|os| has_token(&name, os))
                && (arch_names.iter().any(|arch| has_token(&name, arch))
                    || (os == "macos" && has_token(&name, "universal")))
        })
        .collect();
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    match candidates.as_slice() {
        [asset] => Ok(*asset),
        [] => Err(format!(
            "no GitHub release asset matches {}; specify `asset` explicitly",
            context.system
        )),
        _ => Err(format!(
            "ambiguous GitHub release assets for {}: {}; specify `asset` explicitly",
            context.system,
            candidates
                .iter()
                .map(|asset| asset.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn has_token(name: &str, token: &str) -> bool {
    name.match_indices(token).any(|(start, _)| {
        let end = start + token.len();
        (start == 0 || !name.as_bytes()[start - 1].is_ascii_alphanumeric())
            && (end == name.len() || !name.as_bytes()[end].is_ascii_alphanumeric())
    })
}

fn discover_bins(root: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut bins = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let mut entries = fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !kind.is_file()
                || entry
                    .metadata()
                    .map_err(|e| e.to_string())?
                    .permissions()
                    .mode()
                    & 0o111
                    == 0
            {
                continue;
            }
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 binary name")?;
            if bins.insert(name.clone(), relative).is_some() {
                return Err(format!(
                    "multiple binaries named `{name}`; specify `bins` explicitly"
                ));
            }
        }
    }
    if bins.is_empty() {
        return Err(
            "GitHub archive has no executable files; specify `bins` explicitly".to_string(),
        );
    }
    Ok(bins)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_string(),
            browser_download_url: String::new(),
            digest: None,
        }
    }

    #[test]
    fn selects_native_release_and_ignores_checksums() {
        let assets = vec![
            asset("tool-linux-arm64.tar.gz"),
            asset("tool-darwin-amd64.tar.gz"),
            asset("tool-darwin-arm64.tar.gz"),
            asset("tool-darwin-arm64.tar.gz.sha256"),
        ];
        assert_eq!(
            select_asset(&assets, None, &ResolveContext::new("aarch64-macos"))
                .unwrap()
                .name,
            "tool-darwin-arm64.tar.gz"
        );
    }

    #[test]
    fn rejects_ambiguous_assets_and_allows_exact_override() {
        let assets = vec![
            asset("tool-linux-x86_64-gnu.tar.gz"),
            asset("tool-linux-x86_64-musl.tar.gz"),
        ];
        let context = ResolveContext::new("x86_64-linux");
        assert!(select_asset(&assets, None, &context)
            .unwrap_err()
            .contains("ambiguous"));
        assert!(select_asset(&assets, Some("tool-linux-x86_64-musl.tar.gz"), &context).is_ok());
    }

    #[test]
    fn rejects_wrong_architecture_and_unsupported_installers() {
        let assets = vec![
            asset("tool-darwin-arm64.dmg"),
            asset("tool-darwin-amd64.tar.gz"),
        ];
        let context = ResolveContext::new("aarch64-macos");
        assert!(select_asset(&assets, None, &context).is_err());
        assert!(select_asset(&assets, Some("tool-darwin-arm64.dmg"), &context).is_err());
        assert!(!has_token("tool-arm64e", "arm64"));
    }

    #[test]
    fn resolves_raw_release_with_verified_digest() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("repos/owner/tool/releases/tags");
        fs::create_dir_all(&dir).unwrap();
        let binary = root.path().join("tool");
        fs::write(&binary, b"#!/bin/sh\n").unwrap();
        let digest = hash_bytes(b"#!/bin/sh\n");
        fs::write(
            dir.join("v1"),
            serde_json::to_vec(&serde_json::json!({
                "id": 42, "tag_name": "v1", "assets": [{"name": "tool-darwin-arm64",
                    "browser_download_url": format!("file://{}", binary.display()),
                    "digest": format!("sha256:{digest}")}]
            }))
            .unwrap(),
        )
        .unwrap();
        let resolver = GitHubResolver {
            api_url: format!("file://{}", root.path().display()),
            downloads: DownloadCache::new(root.path().join("downloads")),
        };
        let request = PackageRequest::parse("github:owner/tool@v1");
        let resolution = resolver
            .resolve(&request, &ResolveContext::new("aarch64-macos"))
            .unwrap()
            .unwrap();
        assert_eq!(
            resolution.package.provides.bins["tool"],
            PathBuf::from("tool")
        );
        assert!(matches!(
            resolution.proof,
            ResolutionProof::GitRelease(GitReleaseProof {
                release_id: Some(42),
                ..
            })
        ));
        fs::write(&binary, b"changed").unwrap();
        fs::remove_dir_all(root.path().join("downloads")).unwrap();
        assert!(resolver
            .resolve(&request, &ResolveContext::new("aarch64-macos"))
            .unwrap_err()
            .contains("hash mismatch"));
    }

    #[test]
    fn discovers_binary_names_and_rejects_collisions() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("release/bin")).unwrap();
        let path = root.path().join("release/bin/rg");
        fs::write(&path, b"binary").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(root.path().join("README"), b"docs").unwrap();
        assert_eq!(
            discover_bins(root.path()).unwrap()["rg"],
            PathBuf::from("release/bin/rg")
        );
        fs::copy(path, root.path().join("rg")).unwrap();
        assert!(discover_bins(root.path())
            .unwrap_err()
            .contains("multiple binaries"));
    }
}
