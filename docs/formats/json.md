# json

JSON codec. Output is pretty-printed with 2-space indent and a trailing
newline. See the [formats overview](./) for the shared codec shape and
encoding rules common to every format.

## Examples

Materialize a settings file:

```lua
local rb = require("rootbeer")

rb.json.write("~/.config/myapp/settings.json", {
  theme = "dark",
  recent = { "a.txt", "b.txt" },
})
```

Patch a single field in place:

```lua
local cfg = rb.json.read("~/.config/myapp/settings.json")
cfg.theme = "light"
rb.json.write("~/.config/myapp/settings.json", cfg)
```

Encode without touching disk:

```lua
local payload = rb.json.encode({ ok = true, data = { 1, 2, 3 } })
```

## Notes

- `NaN` and `Infinity` are rejected — JSON cannot represent them. Filter
  these out of your tables before encoding.

## API

<!--@include: ../api/_generated/json.md-->
