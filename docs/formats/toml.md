# toml

TOML codec. See the [formats overview](./) for the shared codec shape and
encoding rules common to every format.

## Examples

Generate a config:

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

## Notes

- Arrays of scalars use inline syntax (`features = ["fast", "secure"]`);
  arrays of tables use `[[array]]` syntax.
- TOML datetimes are decoded as strings — bring your own date library if
  you need to manipulate them.

## API

<!--@include: ../api/_generated/toml.md-->
