use mlua::{Function, Table, Value};

use crate::profile::error::ProfileError;
use crate::profile::name;

pub struct Setup {
    pub strategy: Strategy,
    pub profiles: Vec<(String, Spec)>,
}

pub struct Spec {
    pub hosts: Vec<String>,
    pub users: Vec<String>,
}

pub enum Strategy {
    Cli,
    Hostname,
    User,
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
        for pair in profiles_table.pairs::<String, Table>() {
            let (name_, spec_table) = pair?;
            name::validate(&name_).map_err(|reason| ProfileError::InvalidName {
                name: name_.clone(),
                reason,
            })?;
            profiles.push((name_, Spec::from_lua(spec_table)?));
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
            Value::String(s) => match s.to_str()?.as_ref() {
                "cli" => Ok(Self::Cli),
                "hostname" => Ok(Self::Hostname),
                "user" => Ok(Self::User),
                other => Err(ProfileError::InvalidStrategy(other.to_string()).into()),
            },
            Value::Function(f) => Ok(Self::Custom(f)),
            v => Err(mlua::Error::runtime(format!(
                "strategy must be a string or function, got {}",
                v.type_name()
            ))),
        }
    }
}

impl Spec {
    fn from_lua(table: Table) -> mlua::Result<Self> {
        Ok(Self {
            hosts: read_string_array(&table, "hosts")?,
            users: read_string_array(&table, "users")?,
        })
    }
}

fn read_string_array(table: &Table, key: &str) -> mlua::Result<Vec<String>> {
    match table.raw_get::<Value>(key)? {
        Value::Nil => Ok(Vec::new()),
        Value::Table(t) => t.sequence_values::<String>().collect(),
        v => Err(mlua::Error::runtime(format!(
            "'{key}' must be a string array, got {}",
            v.type_name()
        ))),
    }
}
