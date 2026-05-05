use std::collections::BTreeMap;

use mlua::{Function, Lua, Table, Value};

use crate::lua::module::Module;
use crate::profile::error::ProfileError;
use crate::profile::spec::{Setup, Spec, Strategy};
use crate::profile::ProfileContext;

pub(crate) struct Profile;

impl Module for Profile {
    const NAME: &'static str = "profile";

    fn build(lua: &Lua, t: &Table) -> mlua::Result<()> {
        t.set("define", lua.create_function(define)?)?;
        t.set("current", lua.create_function(current)?)?;
        t.set("select", lua.create_function(select)?)?;
        t.set("when", lua.create_function(when)?)?;
        t.set("config", lua.create_function(config)?)?;
        Ok(())
    }
}

fn define(lua: &Lua, spec_table: Table) -> mlua::Result<()> {
    if state(lua).schema().is_some() {
        return Err(ProfileError::AlreadyConfigured.into());
    }

    let Setup { strategy, profiles } = Setup::from_lua(spec_table)?;
    let schema: BTreeMap<String, Spec> = profiles.into_iter().collect();
    state_mut(lua).schema = Some(schema);

    let cli = state(lua).cli().map(String::from);
    if let Some(cli) = cli {
        let known = state(lua).schema_keys();
        if !state(lua).schema().unwrap().contains_key(&cli) {
            return Err(ProfileError::Required {
                active: Some(cli),
                profiles: known,
            }
            .into());
        }
        state_mut(lua).active = Some(cli);
        return Ok(());
    }

    state_mut(lua).active = resolve(lua, strategy)?;
    Ok(())
}

fn resolve(lua: &Lua, strategy: Strategy) -> mlua::Result<Option<String>> {
    let host: Table = lua.globals().get::<Table>("rootbeer")?.get("host")?;
    let hostname: Option<String> = host.get("hostname")?;
    let user: String = host.get("user")?;

    match strategy {
        Strategy::Cli => Err(ProfileError::Required {
            active: None,
            profiles: state(lua).schema_keys(),
        }
        .into()),

        Strategy::Hostname => {
            let matched = hostname
                .as_deref()
                .and_then(|h| state(lua).match_hostname(h));
            matched.map(Some).ok_or_else(|| {
                ProfileError::NoMatch {
                    strategy: "hostname",
                    value: hostname,
                    profiles: state(lua).schema_keys(),
                }
                .into()
            })
        }

        Strategy::User => state(lua).match_user(&user).map(Some).ok_or_else(|| {
            ProfileError::NoMatch {
                strategy: "user",
                value: Some(user),
                profiles: state(lua).schema_keys(),
            }
            .into()
        }),

        Strategy::Custom(f) => {
            let ctx = build_ctx(lua, hostname, user)?;
            match f.call::<Value>(ctx)? {
                Value::Nil => Ok(None),
                Value::String(s) => {
                    let name = s.to_str()?.to_string();
                    if !state(lua).schema().unwrap().contains_key(&name) {
                        return Err(ProfileError::Required {
                            active: Some(name),
                            profiles: state(lua).schema_keys(),
                        }
                        .into());
                    }
                    Ok(Some(name))
                }
                v => Err(ProfileError::StrategyReturnedNonString(v.type_name()).into()),
            }
        }
    }
}

fn build_ctx(lua: &Lua, hostname: Option<String>, user: String) -> mlua::Result<Table> {
    let ctx = lua.create_table()?;

    let h = hostname;
    ctx.set(
        "match_hostname",
        lua.create_function(move |lua, ()| {
            Ok(h.as_deref().and_then(|hn| state(lua).match_hostname(hn)))
        })?,
    )?;

    let u = user;
    ctx.set(
        "match_user",
        lua.create_function(move |lua, ()| Ok(state(lua).match_user(&u)))?,
    )?;

    Ok(ctx)
}

fn current(lua: &Lua, _: ()) -> mlua::Result<Option<String>> {
    let s = state(lua);
    Ok(s.active().or_else(|| s.cli()).map(String::from))
}

fn select(lua: &Lua, map: Table) -> mlua::Result<Value> {
    let s = state(lua);
    let schema = s.schema().ok_or(ProfileError::NotConfigured)?;

    for pair in map.pairs::<String, Value>() {
        let (k, _) = pair?;
        if k != "default" && !schema.contains_key(&k) {
            return Err(ProfileError::Required {
                active: Some(k),
                profiles: schema.keys().cloned().collect(),
            }
            .into());
        }
    }

    if let Some(name) = s.active() {
        let v: Value = map.get(name)?;
        if !matches!(v, Value::Nil) {
            return Ok(v);
        }
    }

    let default: Value = map.get("default")?;
    if !matches!(default, Value::Nil) {
        return Ok(default);
    }

    let map_keys: Vec<String> = map
        .pairs::<String, Value>()
        .filter_map(|p| p.ok().map(|(k, _)| k))
        .collect();
    Err(ProfileError::Required {
        active: s.active().map(String::from),
        profiles: map_keys,
    }
    .into())
}

fn when(lua: &Lua, (names, fn_): (Value, Function)) -> mlua::Result<()> {
    let s = state(lua);
    let schema = s.schema().ok_or(ProfileError::NotConfigured)?;

    let names_vec: Vec<String> = match names {
        Value::String(s) => vec![s.to_str()?.to_string()],
        Value::Table(t) => t.sequence_values::<String>().collect::<mlua::Result<_>>()?,
        v => {
            return Err(mlua::Error::runtime(format!(
                "when: 'names' must be a string or array, got {}",
                v.type_name()
            )))
        }
    };

    for n in &names_vec {
        if !schema.contains_key(n) {
            return Err(ProfileError::Required {
                active: Some(n.clone()),
                profiles: schema.keys().cloned().collect(),
            }
            .into());
        }
    }

    let active = s.active().map(String::from);
    drop(s);

    if let Some(active) = active {
        if names_vec.iter().any(|n| n == &active) {
            fn_.call::<()>(())?;
        }
    }
    Ok(())
}

fn config(lua: &Lua, map: Table) -> mlua::Result<()> {
    let s = state(lua);
    let schema = s.schema().ok_or(ProfileError::NotConfigured)?;

    for pair in map.pairs::<String, String>() {
        let (k, _) = pair?;
        if !schema.contains_key(&k) {
            return Err(ProfileError::Required {
                active: Some(k),
                profiles: schema.keys().cloned().collect(),
            }
            .into());
        }
    }

    let rb: Table = lua.globals().get("rootbeer")?;
    let is_file: Function = rb.get("is_file")?;
    for pair in map.pairs::<String, String>() {
        let (name, path) = pair?;
        if !is_file.call::<bool>(path.clone())? {
            return Err(mlua::Error::runtime(format!(
                "profile '{name}': file not found: {path}"
            )));
        }
    }

    let active = s.active().map(String::from);
    drop(s);

    if let Some(active) = active {
        if let Ok(path) = map.get::<String>(active) {
            let module = path.strip_suffix(".lua").unwrap_or(&path).to_string();
            let require: Function = lua.globals().get("require")?;
            require.call::<Value>(module)?;
        }
    }
    Ok(())
}

fn state(lua: &Lua) -> mlua::AppDataRef<'_, ProfileContext> {
    lua.app_data_ref::<ProfileContext>()
        .expect("ProfileState not registered; call ProfileState::new in vm setup")
}

fn state_mut(lua: &Lua) -> mlua::AppDataRefMut<'_, ProfileContext> {
    lua.app_data_mut::<ProfileContext>()
        .expect("ProfileState not registered; call ProfileState::new in vm setup")
}
