# Data Formats

Rootbeer ships built-in codecs for the configuration formats you actually
encounter in dotfiles. Every codec is a sub-table on `rb` with the same
four-function shape:

```lua
rb.<fmt>.encode(t)        -- table  → string
rb.<fmt>.decode(s)        -- string → table
rb.<fmt>.read(path)       -- path   → table   (slurp ∘ decode)
rb.<fmt>.write(path, t)   -- path, table → () (encode ∘ file)
```

`encode` and `decode` are pure transformations. `read` slurps a file
synchronously at plan time — the file must exist when your script runs.
`write` is **deferred**: it appends a `WriteFile` op that runs during
`rb apply`, exactly like `rb.file()`. A trailing newline is always added
on `write` so the output is well-formed.

## Available codecs

| Codec               | Read  | Write | Notes                                          |
| ------------------- | :---: | :---: | ---------------------------------------------- |
| [`json`](./json)    |   ✓   |   ✓   | Pretty-printed with 2-space indent.            |
| [`toml`](./toml)    |   ✓   |   ✓   | Datetimes decode as strings.                   |
| [`yaml`](./yaml)    |   ✓   |   ✓   | Tags decode transparently to scalars.          |
| [`plist`](./plist)  |   ✓   |   ✓   | XML output; decode accepts XML or binary.      |

## Encoding rules

These apply to every codec. Format-specific behaviour is documented on
each codec's page.

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
