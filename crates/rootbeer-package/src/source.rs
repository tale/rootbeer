use serde::{Deserialize, Serialize};

/// An explicit source-build request, separate from the approved release version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SourceSelection {
    Release,
    Head,
    Tag(String),
    Branch(String),
    Revision(String),
}

impl SourceSelection {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Revision(value)
                if value.len() != 40
                    || !value
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) =>
            {
                Err("source revisions require a full lowercase Git commit SHA".into())
            }
            Self::Tag(value) | Self::Branch(value)
                if value.is_empty()
                    || value.starts_with('-')
                    || value.chars().any(|c| c.is_whitespace() || c.is_control())
                    || value.contains(['~', '^', ':', '?', '*', '[', '\\'])
                    || value.contains("..")
                    || value.contains("@{") =>
            {
                Err("source tags and branches must be literal Git ref names".into())
            }
            _ => Ok(()),
        }
    }
}

/// Repository authorized by a source recipe for development builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitSource {
    pub github: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

impl GitSource {
    pub fn validate(&self) -> Result<(), String> {
        crate::github::repository(&self.github)?;
        if let Some(branch) = &self.branch {
            SourceSelection::Branch(branch.clone()).validate()?;
        }
        Ok(())
    }
}

/// Immutable provenance for a package built from local recipes or a package repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBuildProof {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<crate::RepositoryPin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_catalog_sha256: Option<String>,
    pub catalog_sha256: String,
    pub recipe_sha256: String,
    pub source_url: String,
    pub source_sha256: String,
    pub git_commit: Option<String>,
    pub build_key: String,
    pub receipt_sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackageRequest;

    #[test]
    fn selectors_roundtrip_without_becoming_backend_names_or_release_versions() {
        for text in [
            "tool@HEAD",
            "tool@tag:v1",
            "tool@branch:feature/work",
            "tool@source:1",
            "tool@source:default",
            &format!("tool@rev:{}", "a".repeat(40)),
        ] {
            let request = PackageRequest::parse(text);
            request.source.as_ref().unwrap().validate().unwrap();
            assert_eq!(request.to_string(), text);
            assert_eq!(PackageRequest::parse(&request.to_string()), request);
            assert!(request.resolver.is_none());
        }
        assert!(PackageRequest::parse("github:owner/tool@HEAD")
            .source
            .is_none());
        for selection in [
            SourceSelection::Revision("abc123".into()),
            SourceSelection::Tag("v1^{}".into()),
            SourceSelection::Branch("../main".into()),
        ] {
            assert!(selection.validate().is_err());
        }
    }
}
