use std::fs;
use std::process::Command;

use rootbeer_package::download::DownloadCache;
use rootbeer_package::index::IndexResolver;
use rootbeer_package::*;
use rootbeer_store::{hash_bytes, hash_file};

/// Builds an installation resolver that prefers published artifacts and can execute source recipes.
pub fn resolver_stack_for_inputs(inputs: &PackageResolverInputs) -> ResolverStack {
    let mut stack = backend_stack(inputs).with_implicit_resolver("rootbeer");
    if let Some(pin) = inputs.package_index() {
        stack.push(SourceResolver::new(pin, rootbeer_store::state_dir()));
    }
    stack
}

/// Installs published artifacts or builds recipes from a verified index.
pub struct SourceResolver {
    state: std::path::PathBuf,
    index: IndexResolver,
    pin: PackageIndexPin,
}

impl SourceResolver {
    /// Keeps source downloads, build results, and diagnostics under the caller's state directory.
    pub fn new(pin: &PackageIndexPin, state: impl Into<std::path::PathBuf>) -> Self {
        let state = state.into();
        Self {
            index: IndexResolver::with_cache(pin, state.join("downloads")),
            pin: pin.clone(),
            state,
        }
    }
}

impl PackageResolver for SourceResolver {
    fn name(&self) -> &str {
        "rootbeer"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        if request.asset.is_some() || !request.bins.is_empty() {
            return Err("index packages do not accept asset or command overrides".into());
        }
        if let Some(selection) = &request.source {
            selection.validate()?;
            if request.version.is_some() && !matches!(selection, SourceSelection::Release) {
                return Err("Git refs cannot be combined with a release version".into());
            }
        }
        let index = self.index.index()?;
        let package = index
            .catalog
            .find(&request.name)
            .ok_or_else(|| format!("unknown index package `{}`", request.name))?;
        let version = request
            .version
            .as_deref()
            .unwrap_or_else(|| package.default_version_for(&context.system));
        let key = format!("{}@{version}", package.name);
        if request.source.is_none()
            && index
                .artifacts
                .get(&key)
                .is_some_and(|systems| systems.contains_key(&context.system))
        {
            return self.index.resolve(request, context);
        }
        if context != &ResolveContext::current() {
            return Err("local source builds require the host platform".into());
        }
        let original = package
            .versions
            .get(version)
            .ok_or_else(|| format!("{key}: no approved recipe"))?;
        if !original.systems.contains(&context.system) {
            return Err(format!("{key}: no source recipe for {}", context.system));
        }
        let mut recipe = original.clone();
        let build = recipe
            .build
            .as_mut()
            .ok_or_else(|| format!("{key} is prebuilt-only; no source build is declared"))?;
        let recipe_sha256 =
            hash_bytes(&serde_json::to_vec(original).map_err(|error| error.to_string())?);
        let state = &self.state;
        let downloads = state.join("downloads");
        let mut version = version.to_string();
        let mut git_commit = None;
        if let Some(selection) = request
            .source
            .as_ref()
            .filter(|selection| !matches!(selection, SourceSelection::Release))
        {
            let git = build.git.as_ref().ok_or("this source recipe does not declare a Git repository for HEAD, tag, or revision builds")?;
            let commit = resolve_commit(git, selection)?;
            let (_, repository) = github::repository(&git.github)?;
            build.url = format!("https://github.com/{}/archive/{commit}.tar.gz", git.github);
            build.sha256 = DownloadCache::new(&downloads)
                .materialize(&build.url, None)
                .map_err(|error| error.to_string())?
                .sha256;
            build.archive = ArchiveFormat::TarGz;
            build.strip_prefix = format!("{repository}-{commit}").into();
            version = format!("git-{commit}");
            git_commit = Some(commit);
        }
        build.validate()?;
        let source_url = build.url.clone();
        let source_sha256 = build.sha256.clone();
        recipe.source = None;
        recipe.assets.clear();
        recipe.checksums.clear();
        recipe.bin_paths.clear();
        recipe.mirror = false;
        let mut catalog = index.catalog.clone();
        catalog
            .packages
            .get_mut(&package.name)
            .unwrap()
            .versions
            .insert(version.clone(), recipe);
        catalog.validate()?;
        let runs = state.join("source-builds");
        fs::create_dir_all(&runs).map_err(|error| error.to_string())?;
        let run = tempfile::tempdir_in(&runs)
            .map_err(|error| error.to_string())?
            .keep();
        let artifact = crate::build_package(
            &catalog,
            &format!("{}@{version}", package.name),
            &run.join("output"),
            &crate::BuildOptions {
                downloads,
                cache: Some(crate::BuildCache {
                    directory: state.join("builds"),
                    context: "consumer-v1".into(),
                    recheck: false,
                }),
                ..Default::default()
            },
        )?;
        let proof = SourceBuildProof {
            index: self.pin.clone(),
            catalog_sha256: index.catalog_sha256.clone(),
            recipe_sha256,
            source_url,
            source_sha256,
            git_commit,
            build_key: artifact
                .build_key
                .clone()
                .ok_or("source build did not record its cache identity")?,
            receipt_sha256: hash_file(run.join("output/receipt.json"))
                .map_err(|error| error.to_string())?,
        };
        Ok(Some(PackageResolution::new(
            artifact.package,
            ResolutionProof::SourceBuild(proof),
        )))
    }
}

fn resolve_commit(source: &GitSource, selection: &SourceSelection) -> Result<String, String> {
    source.validate()?;
    selection.validate()?;
    if let SourceSelection::Revision(commit) = selection {
        return Ok(commit.clone());
    }
    let reference = match selection {
        SourceSelection::Head => source
            .branch
            .as_ref()
            .map(|branch| format!("refs/heads/{branch}"))
            .unwrap_or("HEAD".into()),
        SourceSelection::Branch(branch) => format!("refs/heads/{branch}"),
        SourceSelection::Tag(tag) => format!("refs/tags/{tag}"),
        _ => return Err("a Git source selection is required".into()),
    };
    let mut command = Command::new("git");
    command.args([
        "ls-remote",
        "--exit-code",
        &format!("https://github.com/{}.git", source.github),
        &reference,
    ]);
    if matches!(selection, SourceSelection::Tag(_)) {
        command.arg(format!("{reference}^{{}}"));
    }
    let output = command
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("cannot resolve Git source: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot resolve {reference}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    commit_from_refs(&String::from_utf8_lossy(&output.stdout), &reference)
}

fn commit_from_refs(output: &str, reference: &str) -> Result<String, String> {
    let mut commit = None;
    let mut peeled = None;
    for line in output.lines() {
        let Some((sha, name)) = line.split_once('\t') else {
            return Err("invalid Git ref response".into());
        };
        SourceSelection::Revision(sha.into()).validate()?;
        if name == reference {
            commit = Some(sha.to_string());
        }
        if name == format!("{reference}^{{}}") {
            peeled = Some(sha.to_string());
        }
    }
    peeled
        .or(commit)
        .ok_or_else(|| format!("Git ref `{reference}` was not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotated_tags_pin_the_commit_and_ignore_other_refs() {
        let object = "a".repeat(40);
        let commit = "b".repeat(40);
        let refs = format!("{object}\trefs/tags/v1\n{commit}\trefs/tags/v1^{{}}\n");
        assert_eq!(commit_from_refs(&refs, "refs/tags/v1").unwrap(), commit);
        assert!(commit_from_refs(&refs, "refs/heads/v1").is_err());
        assert!(commit_from_refs("short\tHEAD\n", "HEAD").is_err());
    }
}
