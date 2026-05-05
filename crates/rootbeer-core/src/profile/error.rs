use super::NameError;
use std::fmt;

#[derive(Debug, Clone)]
pub enum ProfileError {
    NotConfigured,
    AlreadyConfigured,
    MissingField(&'static str),
    EmptyProfiles,
    InvalidName {
        name: String,
        reason: NameError,
    },
    InvalidStrategy(String),
    Required {
        active: Option<String>,
        profiles: Vec<String>,
    },
    NoMatch {
        strategy: &'static str,
        value: Option<String>,
        profiles: Vec<String>,
    },
    StrategyReturnedNonString(&'static str),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => {
                write!(f, "rb.profile not configured; call rb.profile.define first")
            }
            Self::AlreadyConfigured => write!(f, "rb.profile.define has already been called"),
            Self::MissingField(name) => {
                write!(f, "rb.profile.define: missing required field '{name}'")
            }
            Self::EmptyProfiles => {
                write!(
                    f,
                    "rb.profile.define: 'profiles' must contain at least one entry"
                )
            }
            Self::InvalidName { name, reason } => match reason {
                NameError::Empty => write!(f, "profile name is empty"),
                NameError::NotIdentifier => {
                    write!(f, "profile name '{name}' is not a valid Lua identifier")
                }
                NameError::ReservedKeyword => {
                    write!(f, "profile name '{name}' is a reserved keyword")
                }
            },
            Self::InvalidStrategy(s) => write!(
                f,
                "unknown strategy '{s}', expected 'cli', 'hostname', 'user', or a function"
            ),
            Self::Required { active, profiles } => {
                let known = profiles.join(", ");
                match active {
                    Some(n) => write!(f, "unknown profile '{n}', expected one of: {known}"),
                    None => write!(f, "a profile is required, expected one of: {known}"),
                }
            }
            Self::NoMatch {
                strategy,
                value,
                profiles,
            } => {
                let v = value.as_deref().unwrap_or("<unknown>");
                write!(
                    f,
                    "no profile bound to {strategy} '{v}', known profiles: {}",
                    profiles.join(", ")
                )
            }
            Self::StrategyReturnedNonString(t) => write!(
                f,
                "profile strategy function must return a string or nil, got {t}"
            ),
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<ProfileError> for mlua::Error {
    fn from(e: ProfileError) -> Self {
        mlua::Error::external(e)
    }
}
