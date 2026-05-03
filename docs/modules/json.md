# json

`rb.json` is the JSON writer. Like every Rootbeer writer, it exposes the
same four composable functions:

```lua
rb.json.encode(t)        -- table  → string
rb.json.decode(s)        -- string → table
rb.json.read(path)       -- path   → table   (slurp ∘ decode)
rb.json.write(path, t)   -- path, table → () (encode ∘ file)
```

`encode` / `decode` are pure transformations. `read` slurps a file
synchronously at plan time. `write` is deferred — it appends a `WriteFile`
op that runs during `rb apply`, just like `rb.file()`.

## Examples

Materialize a settings file:

```lua
local rb = require("rootbeer")

rb.json.write("~/.config/myapp/settings.json", {
  theme = "dark",
  recent = { "a.txt", "b.txt" },
})
```

Round-trip an existing config to patch a single field:

```lua
local cfg = rb.json.read("~/.config/myapp/settings.json")
cfg.theme = "light"
rb.json.write("~/.config/myapp/settings.json", cfg)
```

Encode in-memory without touching the filesystem:

```lua
local payload = rb.json.encode({ ok = true, data = { 1, 2, 3 } })
```

## Encoding rules

- Tables with consecutive integer keys starting at 1 become JSON arrays.
- All other tables become objects.
- `nil` values are omitted.
- `NaN` / `Infinity` are rejected — JSON cannot represent them.

## API

<!--@include: ../api/_generated/json.md-->
