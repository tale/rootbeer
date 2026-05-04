# plist

Apple property list codec. Output is **XML plist** — the human-readable
form used by `defaults write` and most files under `~/Library/Preferences/`.
Decoding accepts both XML and binary plists.

See the [formats overview](./) for the shared codec shape and encoding
rules common to every format.

## Examples

Materialize a Library plist:

```lua
local rb = require("rootbeer")

rb.plist.write("~/Library/Preferences/com.example.MyApp.plist", {
  Theme = "Dark",
  RecentDocuments = { "a.md", "b.md" },
  WindowFrame = { x = 100, y = 100, w = 800, h = 600 },
})
```

Patch a single key in an existing plist:

```lua
local cfg = rb.plist.read("~/Library/Preferences/com.example.MyApp.plist")
cfg.Theme = "Light"
rb.plist.write("~/Library/Preferences/com.example.MyApp.plist", cfg)
```

## Notes

- Plist `Date` and `Data` values decode to strings on read; encoding back
  produces a string, not the original typed value.
- Round-tripping a binary plist always re-encodes as XML.

## API

<!--@include: ../api/_generated/plist.md-->
