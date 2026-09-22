use std::time::{Duration, Instant};

use mlua::Lua;
use mlua::LuaSerdeExt;
use serde::Serialize;
use serde_json::Value;

pub fn read<T: serde::de::DeserializeOwned>(source: &str) -> Result<T, String> {
    let (lua, value) = evaluate(source)?;
    lua.from_value(value).map_err(|e| e.to_string())
}

pub(crate) fn evaluate(source: &str) -> Result<(Lua, mlua::Value), String> {
    let lua = Lua::new();
    lua.set_memory_limit(4 * 1024 * 1024)
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    lua.set_interrupt(move |_| {
        if start.elapsed() > Duration::from_secs(1) {
            return Err(mlua::Error::RuntimeError(
                "package definition exceeded execution limit".into(),
            ));
        }
        Ok(mlua::VmState::Continue)
    });
    let environment = lua.create_table().map_err(|e| e.to_string())?;
    let value = lua
        .load(source)
        .set_environment(environment)
        .eval()
        .map_err(|e| e.to_string())?;
    Ok((lua, value))
}

pub(crate) fn write(value: &impl Serialize) -> Result<String, String> {
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    Ok(format!("return {}\n", render(&value, 0)))
}

fn quote(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            character if character.is_ascii_control() => {
                output.push_str(&format!("\\{:03}", character as u32))
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn render(value: &Value, depth: usize) -> String {
    render_field(value, depth, 0)
}

fn render_field(value: &Value, depth: usize, prefix: usize) -> String {
    match value {
        Value::Null => "nil".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => quote(value),
        Value::Array(values) if values.is_empty() => "{}".into(),
        Value::Object(values) if values.is_empty() => "{}".into(),
        Value::Array(values) => {
            if values
                .iter()
                .all(|value| !value.is_array() && !value.is_object())
            {
                let inline = format!(
                    "{{ {} }}",
                    values
                        .iter()
                        .map(|value| render(value, depth + 1))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                if depth * 4 + prefix + inline.len() < 100 {
                    return inline;
                }
            }
            let entries = values.iter().map(|value| render(value, depth + 1));
            table(entries, depth)
        }
        Value::Object(values) => {
            // A flattened recipe reads `{}` back as a map, so an empty list has no spelling
            // that survives; every list field defaults, so omitting it means the same thing.
            let entries = values.iter().filter(|(_, value)| !is_empty_list(value));
            let entries = entries.map(|(key, value)| {
                let key = field(key);
                format!("{key} = {}", render_field(value, depth + 1, key.len() + 3))
            });
            table(entries, depth)
        }
    }
}

fn is_empty_list(value: &Value) -> bool {
    value.as_array().is_some_and(Vec::is_empty)
}

fn field(key: &str) -> String {
    let is_identifier = key
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let is_reserved = matches!(
        key,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
            | "continue"
            | "type"
            | "export"
    );
    if is_identifier && !is_reserved {
        return key.into();
    }
    format!("[{}]", quote(key))
}

fn table(entries: impl Iterator<Item = String>, depth: usize) -> String {
    let indent = "    ".repeat(depth + 1);
    let entries: Vec<_> = entries.map(|entry| format!("{indent}{entry},")).collect();
    format!("{{\n{}\n{}}}", entries.join("\n"), "    ".repeat(depth))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_untrusted_text_without_executing_it() {
        let input = std::collections::BTreeMap::from([
            (
                "description",
                "Unicode café\n\0\u{7f}123\"; error('bad') --",
            ),
            ("key\\\"", "\\tag{version}"),
            ("end", "reserved Lua keyword"),
        ]);
        let output: std::collections::BTreeMap<String, String> =
            read(&write(&input).unwrap()).unwrap();
        for (key, value) in input {
            assert_eq!(output[key], value);
        }
        assert!(read::<String>("return os.getenv('HOME')").is_err());
    }
}
