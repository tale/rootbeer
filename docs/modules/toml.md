# toml

`rb.toml` is the TOML writer. It mirrors the [json](./json) shape:

```lua
rb.toml.encode(t)        -- table  → string
rb.toml.decode(s)        -- string → table
rb.toml.read(path)       -- path   → table   (slurp ∘ decode)
rb.toml.write(path, t)   -- path, table → () (encode ∘ file)
```

## Examples

Generate a `Cargo`-style config:

```lua
local rb = require("rootbeer")

rb.toml.write("~/.config/myapp/config.toml", {
  app = {
    name = "rootbeer",
    workers = 4,
  },
  features = { "fast", "secure" },
})
```

Read and patch:

```lua
local cfg = rb.toml.read("~/.cargo/config.toml")
cfg.build = cfg.build or {}
cfg.build.jobs = 8
rb.toml.write("~/.cargo/config.toml", cfg)
```

## Encoding rules

- Tables with consecutive integer keys starting at 1 become arrays. Arrays
  of scalars use inline syntax; arrays of tables use `[[array]]` syntax.
- Other tables become `[section]` headers.
- TOML datetimes returned by `decode` are surfaced as strings — bring your
  own date library if you need to manipulate them.

## API

<!--@include: ../api/_generated/toml.md-->
