# yaml

YAML codec. See the [formats overview](./) for the shared codec shape and
encoding rules common to every format.

## Examples

Materialize a config:

```lua
local rb = require("rootbeer")

rb.yaml.write("~/.config/myapp/config.yaml", {
  theme = "dracula",
  plugins = { "git", "format-on-save" },
})
```

Read and patch:

```lua
local cfg = rb.yaml.read("~/.config/myapp/config.yaml")
cfg.theme = "tokyonight"
rb.yaml.write("~/.config/myapp/config.yaml", cfg)
```

## Notes

- Mapping keys are always emitted as strings; non-string source keys are
  stringified.
- Tagged YAML values (e.g. `!!str`) decode transparently to the underlying
  scalar.

## API

<!--@include: ../api/_generated/yaml.md-->
