use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageResolverInputs {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resolvers: BTreeMap<String, ResolverInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolverInput {
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

    pub fn catalog_sha256(&self) -> Option<&str> {
        match self.resolvers.get("rootbeer") {
            Some(ResolverInput::Catalog { sha256 }) => Some(sha256),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inputs_are_empty() {
        assert!(PackageResolverInputs::default().is_empty());
    }
}
