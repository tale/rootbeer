//! Writer utilities for data formats and scripts.
//!
//! Each format implements a `Codec` trait by providing `encode`/`decode`
//! over a Serde Value (makes it easy to implement new formats). The codec
//! exposes a register method to easily wire it up to the `rb` table.
//!
//! ```text
//! rb.<fmt>.encode(t)        — table -> string
//! rb.<fmt>.decode(s)        — string -> table
//! rb.<fmt>.read(path)       — path  -> table (consume and decode)
//! rb.<fmt>.write(path, t)   — path, table -> (encode and write)
//! ```
//!
//! `read` is synchronous (the script needs the value immediately). `write`
//! defers via to the planning architecture of Rootbeer and only runs during
//! the execution phase.
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

    pub fn match_hostname(&self, hostname: &str) -> Option<String> {
        self.schema.as_ref().and_then(|schema| {
            schema
                .iter()
                .find(|(_, spec)| spec.hosts.iter().any(|h| h == hostname))
                .map(|(name, _)| name.clone())
        })
    }

    pub fn match_user(&self, user: &str) -> Option<String> {
        self.schema.as_ref().and_then(|schema| {
            schema
                .iter()
                .find(|(_, spec)| spec.users.iter().any(|u| u == user))
                .map(|(name, _)| name.clone())
        })
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
