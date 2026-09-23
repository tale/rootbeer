pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn validate_https(url: &str) -> Result<(), String> {
    let uri: http::Uri = url.parse().map_err(|e| format!("invalid URL: {e}"))?;
    if uri.scheme_str() != Some("https")
        || uri.host().is_none_or(str::is_empty)
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '@' | '#' | '\\'))
    {
        return Err("PDR and artifact URLs must use HTTPS without credentials or fragments".into());
    }
    Ok(())
}
