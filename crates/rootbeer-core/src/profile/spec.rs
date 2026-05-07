use std::collections::BTreeMap;

use mlua::{Function, Table, Value};

use crate::profile::error::ProfileError;
use crate::profile::name;

pub struct Setup {
    pub strategy: Strategy,
    pub profiles: Vec<(String, Spec)>,
}

pub struct Spec {
    pub matches: Vec<String>,
}

pub enum Strategy {
    Builtin(String),
    Custom(Function),
}

impl Setup {
    pub fn from_lua(table: Table) -> mlua::Result<Self> {
        let strategy = match table.raw_get::<Value>("strategy")? {
            Value::Nil => return Err(ProfileError::MissingField("strategy").into()),
            v => Strategy::from_lua(v)?,
        };

        let profiles_table = match table.raw_get::<Value>("profiles")? {
            Value::Nil => return Err(ProfileError::MissingField("profiles").into()),
            Value::Table(t) => t,
            v => {
                return Err(mlua::Error::runtime(format!(
                    "'profiles' must be a table, got {}",
                    v.type_name()
                )))
            }
        };

        let mut profiles = Vec::new();
        for pair in profiles_table.pairs::<String, Value>() {
            let (name_, spec_value) = pair?;
            name::validate(&name_).map_err(|reason| ProfileError::InvalidName {
                name: name_.clone(),
                reason,
            })?;
            let spec = Spec::from_lua(spec_value, &name_)?;
            profiles.push((name_, spec));
        }

        if profiles.is_empty() {
            return Err(ProfileError::EmptyProfiles.into());
        }

        Ok(Self { strategy, profiles })
    }
}

impl Strategy {
    fn from_lua(value: Value) -> mlua::Result<Self> {
        match value {
            Value::String(s) => Ok(Self::Builtin(s.to_str()?.to_string())),
            Value::Function(f) => Ok(Self::Custom(f)),
            v => Err(mlua::Error::runtime(format!(
                "strategy must be a string or function, got {}",
                v.type_name()
            ))),
        }
    }
}

impl Spec {
    fn from_lua(value: Value, name: &str) -> mlua::Result<Self> {
        let table = match value {
            Value::Table(t) => t,
            v => {
                return Err(mlua::Error::runtime(format!(
                    "profile '{name}' matchers must be a string array, got {}",
                    v.type_name()
                )))
            }
        };

        Ok(Self {
            matches: read_matchers(&table, name)?,
        })
    }
}

fn read_matchers(table: &Table, name: &str) -> mlua::Result<Vec<String>> {
    let mut matchers = BTreeMap::new();

    for pair in table.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let idx = match key {
            Value::Integer(i) if i > 0 => i as usize,
            _ => {
                return Err(mlua::Error::runtime(format!(
                    "profile '{name}' matchers must be an array of strings"
                )))
            }
        };

        match value {
            Value::String(s) => {
                matchers.insert(idx, s.to_str()?.to_string());
            }
            v => {
                return Err(mlua::Error::runtime(format!(
                    "profile '{name}' matcher #{idx} must be a string, got {}",
                    v.type_name()
                )))
            }
        }
    }

    Ok(matchers.into_values().collect())
}
