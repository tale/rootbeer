# Data Formats

Read and write JSON, TOML, YAML, and plist files from your configuration.
Each format provides `encode`, `decode`, `read`, and `write` functions.

```lua
local rb = require("rootbeer")

rb.json.write("~/.config/myapp/settings.json", { theme = "dark" })
```

`read` loads an existing file when your config runs. `write` saves the file when
you apply it. Use `encode` and `decode` to convert between Lua values and text
without reading or writing a file.

## Available formats

| Format              | Read  | Write | Notes                                          |
| ------------------- | :---: | :---: | ---------------------------------------------- |
| [`json`](./json)    |   ✓   |   ✓   | Pretty-printed with 2-space indent.            |
| [`toml`](./toml)    |   ✓   |   ✓   | Datetimes decode as strings.                   |
| [`yaml`](./yaml)    |   ✓   |   ✓   | Tags decode transparently to scalars.          |
| [`plist`](./plist)  |   ✓   |   ✓   | XML output; decode accepts XML or binary.      |

## Encoding rules

These apply to every format. Format-specific behaviour is documented on
each format's page.

- Tables with consecutive integer keys starting at `1` become arrays /
  sequences. All other tables become objects / maps / dictionaries.
- `nil` values are omitted from the output.
- `NaN` and `Infinity` are rejected by formats that cannot represent
  them (JSON, plist).
- Lua functions, threads, and userdata error on encode — they have no
  serialized form.

## Choosing format by extension

Three writes, one config table:

```lua
local data = { theme = "dark", recent = { "a.txt" } }

rb.yaml.write("~/.config/app/config.yaml", data)
rb.json.write("~/.config/app/config.json", data)
rb.toml.write("~/.config/app/config.toml", data)
```
