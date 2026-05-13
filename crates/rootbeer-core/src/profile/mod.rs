//! Profile resolution and branching helpers.
//!
//! The user declares valid profile names and the string matchers that resolve
//! to each profile. Strategies decide which runtime string should be matched
//! against that schema when the CLI did not provide an explicit profile.
//!

mod error;
mod lua;
mod name;
mod spec;

use std::collections::BTreeMap;

pub use error::ProfileError;
pub(crate) use lua::Profile;
pub use name::NameError;
pub use spec::{Spec, Strategy};

pub struct ProfileContext {
    cli: Option<String>,
    schema: Option<BTreeMap<String, Spec>>,
    active: Option<String>,
}

impl ProfileContext {
    pub fn new(cli: Option<String>) -> Self {
        Self {
            cli,
            schema: None,
            active: None,
        }
    }

    pub fn cli(&self) -> Option<&str> {
        self.cli.as_deref()
    }

    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn schema(&self) -> Option<&BTreeMap<String, Spec>> {
        self.schema.as_ref()
    }

    pub fn schema_keys(&self) -> Vec<String> {
        self.schema
            .as_ref()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn match_value(&self, value: &str) -> Option<String> {
        self.schema.as_ref().and_then(|schema| {
            schema
                .iter()
                .find(|(_, spec)| spec.matches.iter().any(|m| m == value))
                .map(|(name, _)| name.clone())
        })
    }

    pub fn match_hostname(&self, hostname: &str) -> Option<String> {
        self.match_value(hostname)
    }

    pub fn match_user(&self, user: &str) -> Option<String> {
        self.match_value(user)
    }

    /// The unique profile with an empty matcher list, if exactly one exists.
    /// Used by built-in strategies as an automatic fallback when no matcher
    /// resolves. If multiple profiles have empty matchers the choice is
    /// ambiguous and no fallback applies.
    pub fn fallback(&self) -> Option<String> {
        let schema = self.schema.as_ref()?;
        let mut empties = schema.iter().filter(|(_, spec)| spec.matches.is_empty());
        let first = empties.next()?;
        if empties.next().is_some() {
            return None;
        }
        Some(first.0.clone())
    }
}

/// Try and recursively extract a ProfileError from an `mlua:Error`. These kinds
/// of errors can be wrapped in several layers, hence the looping.
pub fn extract(err: &mlua::Error) -> Option<ProfileError> {
    let mut current = err;
    loop {
        match current {
            mlua::Error::CallbackError { cause, .. } => current = cause.as_ref(),
            mlua::Error::WithContext { cause, .. } => current = cause.as_ref(),
            mlua::Error::ExternalError(arc) => {
                return arc.downcast_ref::<ProfileError>().cloned();
            }

            _ => return None,
        }
    }
}
