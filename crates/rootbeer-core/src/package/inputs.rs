use std::collections::BTreeMap;
use std::io;

use serde::{Deserialize, Serialize};

use super::download::read_url;

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

    pub fn is_empty(&self) -> bool {
        self.resolvers.is_empty()
    }

    pub fn aqua_registry(&self) -> Option<&GitHubRepositoryPin> {
        match self.resolvers.get("aqua") {
            Some(ResolverInput::AquaRegistry(pin)) => Some(pin),
            None => None,
        }
    }
}

#[derive(Deserialize)]
struct GitHubCommitResponse {
    sha: String,
}

fn resolve_github_commit(owner: &str, repo: &str, reference: &str) -> io::Result<String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{reference}");
    let bytes = read_url(&url)?;
    let response: GitHubCommitResponse = serde_json::from_slice(&bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse GitHub commit response from {url}: {e}"),
        )
    })?;

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
