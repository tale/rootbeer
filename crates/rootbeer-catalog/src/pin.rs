use serde::{Deserialize, Serialize};

/// An immutable document selected by content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIndexPin {
    pub url: String,
    pub sha256: String,
}

impl PackageIndexPin {
    /// Validates an HTTPS index or an explicitly selected absolute local file URL.
    pub fn validate(&self) -> Result<(), String> {
        if !is_sha256(&self.sha256) {
            return Err("package index requires a lowercase SHA-256".into());
        }
        if self.url.starts_with("file:///") && !self.url.contains(['?', '#', '\0']) {
            return Ok(());
        }
        validate_https(&self.url)
    }
}

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn validate_https(url: &str) -> Result<(), String> {
    let uri: http::Uri = url.parse().map_err(|e| format!("invalid index URL: {e}"))?;
    if uri.scheme_str() != Some("https")
        || uri.host().is_none_or(str::is_empty)
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '@' | '#' | '\\'))
    {
        return Err(
            "index and artifact URLs must use HTTPS without credentials or fragments".into(),
        );
    }
    Ok(())
}
