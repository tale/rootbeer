# Core API

The core module provides the low-level primitives that all other modules build
on — writing files, creating symlinks, and serializing data formats.

For system information, see [`rb.host`](/reference/host).
For secrets (1Password and friends), see [Secrets](/reference/secrets).
For per-machine configuration, see [Profiles](/guide/profiles).
For managed packages and lockfile modes, see [Packages](/guide/packages).

```lua
local rb = require("rootbeer")
```

## `rb.profile`

The first-class profile system. See the [Profiles guide](/guide/profiles)
for the complete walkthrough. The module exposes:

- `rb.profile.define({ strategy, profiles })` — declare profiles + the
  resolution strategy in one call.
- `rb.profile.current()` — the active profile name (or `nil`).
- `rb.profile.select(map)` / `when(names, fn)` / `config(map)` — branch on
  the active profile.
- Custom strategy functions receive `ctx.match(value)`,
  `ctx.cli()`, `ctx.hostname()`, and `ctx.user()` for explicit strategy
  composition.

## `rb.package`

`rb.package()` declares a tool as part of your desired system configuration.
Canonical names select the official catalog; exact versions and explicit backend
requests are available when needed. Apply records the selection in `rootbeer.lock`.

Use [package search](/packages/) to find declarations and platform support, then
follow the [package guide](/guide/packages) for installation and shell integration.

## API Reference

<!--@include: ../api/_generated/core.md-->
