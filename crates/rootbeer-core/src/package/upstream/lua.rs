use std::time::{Duration, Instant};

use mlua::{Lua, LuaSerdeExt};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

pub(super) fn read<T: DeserializeOwned>(source: &str) -> Result<T, String> {
    let lua = Lua::new();
    lua.set_memory_limit(4 * 1024 * 1024)
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    lua.set_interrupt(move |_| {
        if start.elapsed() > Duration::from_secs(1) {
            return Err(mlua::Error::RuntimeError(
                "upstream definition exceeded execution limit".into(),
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
    lua.from_value(value).map_err(|e| e.to_string())
}

pub(super) fn write(value: &impl Serialize) -> Result<String, String> {
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
    match value {
        Value::Null => "nil".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => quote(value),
        Value::Array(values) if values.is_empty() => "{}".into(),
        Value::Object(values) if values.is_empty() => "{}".into(),
        Value::Array(values) => {
            let entries = values.iter().map(|value| render(value, depth + 1));
            table(entries, depth)
        }
        Value::Object(values) => {
            let entries = values
                .iter()
                .map(|(key, value)| format!("[{}] = {}", quote(key), render(value, depth + 1)));
            table(entries, depth)
        }
    }
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
        ]);
        let output: std::collections::BTreeMap<String, String> =
            read(&write(&input).unwrap()).unwrap();
        for (key, value) in input {
            assert_eq!(output[key], value);
        }
        assert!(read::<String>("return os.getenv('HOME')").is_err());
    }
}
