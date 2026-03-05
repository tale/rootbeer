---
outline: deep
---

# Core API

The core module provides the low-level primitives that all other modules build
on — writing files, creating symlinks, and serializing data formats.

For system information, see [`rb.host`](/api/host).
For per-machine configuration, see [`profile`](/api/profile).

```lua
local rb = require("@rootbeer")
```

## `rb.profile`

`string?` — The active configuration profile, or `nil` when no profile was
passed on the command line. Set via `rb apply <profile>`.

## API Reference

<!--@include: ./_generated/core.md-->
