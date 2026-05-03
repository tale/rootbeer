# ini

`rb.ini` is a minimal INI / gitconfig writer. Unlike [json](./json) and
[toml](./toml), it is **write-only** — Rootbeer ships an emitter aimed at
gitconfig-style files but no parser.

```lua
rb.ini.encode(t)        -- table → string
rb.ini.write(path, t)   -- path, table → ()
```

If you need to round-trip INI, encode it yourself with another tool and
write it via `rb.file()`.

## Examples

```lua
local rb = require("rootbeer")

rb.ini.write("~/.gitconfig", {
  user   = { name = '"Ada"', email = '"ada@example.com"' },
  core   = { editor = '"nvim"' },
  filter = {
    lfs = {
      clean = '"git-lfs clean -- %f"',
      smudge = '"git-lfs smudge -- %f"',
      required = true,
    },
  },
})
```

The high-level [git](./git) module wraps this with proper quoting and a
typed config schema — prefer it for actual gitconfig files.

## Encoding rules

- Top-level keys must be tables; they become `[section]` headers.
- Scalar values within a section emit as `\tkey = value`.
- Nested tables become `[section "subsection"]`.
- No string escaping is performed — quote scalars yourself if the format
  requires it (gitconfig does for strings).

## API

<!--@include: ../api/_generated/ini.md-->
