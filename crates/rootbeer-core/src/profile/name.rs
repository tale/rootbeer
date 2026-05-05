#[derive(Debug, Clone)]
pub enum NameError {
    Empty,
    NotIdentifier,
    ReservedKeyword,
}

/// Best effort attempt to ban Lua keywords in the profile names because the
/// actual profile API uses the profile names as table keys, so we need to make
/// sure that user written matching is still valid Lua.
const RESERVED: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while", "default",
];

pub fn validate(name: &str) -> Result<(), NameError> {
    let mut chars = name.chars();
    let first = chars.next().ok_or(NameError::Empty)?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(NameError::NotIdentifier);
    }

    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(NameError::NotIdentifier);
    }

    if RESERVED.contains(&name) {
        return Err(NameError::ReservedKeyword);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_identifiers() {
        assert!(validate("work").is_ok());
        assert!(validate("work_laptop").is_ok());
        assert!(validate("_private").is_ok());
        assert!(validate("a1b2").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(validate(""), Err(NameError::Empty)));
    }

    #[test]
    fn rejects_leading_digit() {
        assert!(matches!(validate("1prod"), Err(NameError::NotIdentifier)));
    }

    #[test]
    fn rejects_hyphen() {
        assert!(matches!(
            validate("my-profile"),
            Err(NameError::NotIdentifier)
        ));
    }

    #[test]
    fn rejects_reserved_keywords() {
        assert!(matches!(validate("if"), Err(NameError::ReservedKeyword)));
        assert!(matches!(validate("end"), Err(NameError::ReservedKeyword)));
        assert!(matches!(
            validate("default"),
            Err(NameError::ReservedKeyword)
        ));
    }
}
