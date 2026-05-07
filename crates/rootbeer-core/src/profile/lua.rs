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

#[derive(Clone)]
struct StrategyFacts {
    hostname: Option<String>,
    user: String,
}

struct BuiltinStrategy {
    name: &'static str,
    aliases: &'static [&'static str],
    resolve: fn(&Lua, &StrategyFacts) -> mlua::Result<Option<String>>,
    missing: fn(&Lua, &StrategyFacts) -> ProfileError,
}

const BUILTIN_STRATEGIES: &[BuiltinStrategy] = &[
    BuiltinStrategy {
        name: "cli",
        aliases: &[],
        resolve: resolve_cli,
        missing: missing_cli,
    },
    BuiltinStrategy {
        name: "hostname",
        aliases: &["match_hostname"],
        resolve: resolve_hostname,
        missing: missing_hostname,
    },
    BuiltinStrategy {
        name: "user",
        aliases: &["match_user"],
        resolve: resolve_user,
        missing: missing_user,
    },
];

fn define(lua: &Lua, spec_table: Table) -> mlua::Result<()> {
    if state(lua).schema().is_some() {
        return Err(ProfileError::AlreadyConfigured.into());
    }

    let Setup { strategy, profiles } = Setup::from_lua(spec_table)?;
    let schema: BTreeMap<String, Spec> = profiles.into_iter().collect();
    state_mut(lua).schema = Some(schema);

    state_mut(lua).active = resolve(lua, strategy)?;
    Ok(())
}

fn resolve(lua: &Lua, strategy: Strategy) -> mlua::Result<Option<String>> {
    let facts = strategy_facts(lua)?;

    match strategy {
        Strategy::Builtin(name) => {
            let builtin = builtin_strategy(&name)
                .ok_or_else(|| ProfileError::InvalidStrategy(name.clone()))?;
            resolve_builtin(lua, &facts, builtin, true)
        }

        Strategy::Custom(f) => resolve_custom(lua, f, facts),
    }
}

fn strategy_facts(lua: &Lua) -> mlua::Result<StrategyFacts> {
    let host: Table = lua.globals().get::<Table>("rootbeer")?.get("host")?;

    Ok(StrategyFacts {
        hostname: host.get("hostname")?,
        user: host.get("user")?,
    })
}

fn builtin_strategy(name: &str) -> Option<&'static BuiltinStrategy> {
    BUILTIN_STRATEGIES
        .iter()
        .find(|strategy| strategy.name == name)
}

fn resolve_builtin(
    lua: &Lua,
    facts: &StrategyFacts,
    builtin: &'static BuiltinStrategy,
    require_match: bool,
) -> mlua::Result<Option<String>> {
    let matched = (builtin.resolve)(lua, facts)?;
    if matched.is_none() && require_match {
        return Err((builtin.missing)(lua, facts).into());
    }

    Ok(matched)
}

fn resolve_cli(lua: &Lua, _: &StrategyFacts) -> mlua::Result<Option<String>> {
    let s = state(lua);
    let Some(cli) = s.cli() else {
        return Ok(None);
    };

    if !s.schema().unwrap().contains_key(cli) {
        return Err(ProfileError::Required {
            active: Some(cli.to_string()),
            profiles: s.schema_keys(),
        }
        .into());
    }

    Ok(Some(cli.to_string()))
}

fn missing_cli(lua: &Lua, _: &StrategyFacts) -> ProfileError {
    ProfileError::Required {
        active: None,
        profiles: state(lua).schema_keys(),
    }
}

fn resolve_hostname(lua: &Lua, facts: &StrategyFacts) -> mlua::Result<Option<String>> {
    Ok(facts
        .hostname
        .as_deref()
        .and_then(|h| state(lua).match_hostname(h)))
}

fn missing_hostname(lua: &Lua, facts: &StrategyFacts) -> ProfileError {
    ProfileError::NoMatch {
        strategy: "hostname",
        value: facts.hostname.clone(),
        profiles: state(lua).schema_keys(),
    }
}

fn resolve_user(lua: &Lua, facts: &StrategyFacts) -> mlua::Result<Option<String>> {
    Ok(state(lua).match_user(&facts.user))
}

fn missing_user(lua: &Lua, facts: &StrategyFacts) -> ProfileError {
    ProfileError::NoMatch {
        strategy: "user",
        value: Some(facts.user.clone()),
        profiles: state(lua).schema_keys(),
    }
}

fn resolve_custom(lua: &Lua, f: Function, facts: StrategyFacts) -> mlua::Result<Option<String>> {
    let ctx = build_ctx(lua, facts)?;
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

fn build_ctx(lua: &Lua, facts: StrategyFacts) -> mlua::Result<Table> {
    let ctx = lua.create_table()?;

    ctx.set(
        "match",
        lua.create_function(|lua, value: Option<String>| {
            Ok(value.as_deref().and_then(|v| state(lua).match_value(v)))
        })?,
    )?;

    for builtin in BUILTIN_STRATEGIES {
        install_ctx_strategy(lua, &ctx, builtin.name, builtin, &facts)?;
        for alias in builtin.aliases {
            install_ctx_strategy(lua, &ctx, alias, builtin, &facts)?;
        }
    }

    Ok(ctx)
}

fn install_ctx_strategy(
    lua: &Lua,
    ctx: &Table,
    name: &'static str,
    builtin: &'static BuiltinStrategy,
    facts: &StrategyFacts,
) -> mlua::Result<()> {
    let facts = facts.clone();
    ctx.set(
        name,
        lua.create_function(move |lua, ()| resolve_builtin(lua, &facts, builtin, false))?,
    )
}

fn current(lua: &Lua, _: ()) -> mlua::Result<Option<String>> {
    let s = state(lua);
    if s.schema().is_some() {
        Ok(s.active().map(String::from))
    } else {
        Ok(s.cli().map(String::from))
    }
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
