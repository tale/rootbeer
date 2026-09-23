use std::collections::BTreeMap;
use std::io;

use serde::{Deserialize, Serialize};

use super::download::read_json_url;

const AQUA_REGISTRY_OWNER: &str = "aquaproj";
const AQUA_REGISTRY_REPO: &str = "aqua-registry";
const AQUA_REGISTRY_REF: &str = "main";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageResolverInputs {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resolvers: BTreeMap<String, ResolverInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolverInput {
    AquaRegistry(GitHubRepositoryPin),
    Catalog { sha256: String },
    LocalCatalog(Box<super::PackageCatalog>),
    Repository(super::RepositoryPin),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubRepositoryPin {
    pub owner: String,
    pub repo: String,
    pub rev: String,
}

impl PackageResolverInputs {
    pub fn resolve_current() -> io::Result<Self> {
        let mut resolvers = BTreeMap::new();
        resolvers.insert(
            "aqua".to_string(),
            ResolverInput::AquaRegistry(GitHubRepositoryPin {
                owner: AQUA_REGISTRY_OWNER.to_string(),
                repo: AQUA_REGISTRY_REPO.to_string(),
                rev: resolve_github_commit(
                    AQUA_REGISTRY_OWNER,
                    AQUA_REGISTRY_REPO,
                    AQUA_REGISTRY_REF,
                )?,
            }),
        );
        Ok(Self { resolvers })
    }

    /// The repository a configuration chose in place of the official one, if any.
    pub fn configured_repository(&self) -> Option<super::Repository> {
        let locked = self.repository()?.repository();
        let official = super::Repository::official().ok().flatten();
        (Some(&locked) != official.as_ref()).then_some(locked)
    }

    /// The exact repository root canonical requests resolve against.
    pub fn repository(&self) -> Option<&super::RepositoryPin> {
        match self.resolvers.get("rootbeer") {
            Some(ResolverInput::Repository(pin)) => Some(pin),
            _ => None,
        }
    }

    pub fn local_catalog(&self) -> Option<&super::PackageCatalog> {
        match self.resolvers.get("local") {
            Some(ResolverInput::LocalCatalog(catalog)) => Some(catalog),
            _ => None,
        }
    }

    pub fn same_package_authority(&self, other: &Self, request: &super::PackageRequest) -> bool {
        if request
            .resolver
            .as_deref()
            .is_some_and(|name| name != "rootbeer")
        {
            return true;
        }
        let has_local = [self, other].iter().any(|inputs| {
            inputs
                .local_catalog()
                .is_some_and(|catalog| catalog.find(&request.name).is_some())
        });
        let repository = |inputs: &Self| inputs.repository().map(super::RepositoryPin::repository);
        repository(self) == repository(other)
            && (!has_local || self.local_catalog() == other.local_catalog())
    }

    pub fn is_empty(&self) -> bool {
        self.resolvers.is_empty()
    }

    pub fn aqua_registry(&self) -> Option<&GitHubRepositoryPin> {
        match self.resolvers.get("aqua") {
            Some(ResolverInput::AquaRegistry(pin)) => Some(pin),
            _ => None,
        }
    }

    pub fn catalog_sha256(&self) -> Option<&str> {
        match self.resolvers.get("rootbeer") {
            Some(ResolverInput::Catalog { sha256 }) => Some(sha256),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct GitHubCommitResponse {
    sha: String,
}

fn resolve_github_commit(owner: &str, repo: &str, reference: &str) -> io::Result<String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{reference}");
    let response: GitHubCommitResponse = read_json_url(&url)?;

    if response.sha.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("GitHub commit response from {url} did not include a SHA"),
        ));
    }

    Ok(response.sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inputs_are_empty() {
        assert!(PackageResolverInputs::default().is_empty());
    }

    #[test]
    fn exposes_aqua_registry_pin() {
        let inputs = PackageResolverInputs {
            resolvers: BTreeMap::from([(
                "aqua".to_string(),
                ResolverInput::AquaRegistry(GitHubRepositoryPin {
                    owner: "owner".to_string(),
                    repo: "repo".to_string(),
                    rev: "abc123".to_string(),
                }),
            )]),
        };

        assert_eq!(inputs.aqua_registry().unwrap().rev, "abc123");
    }
}
