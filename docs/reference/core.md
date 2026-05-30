# Core API

The core module provides the low-level primitives that all other modules build
on — writing files, creating symlinks, and serializing data formats.

For system information, see [`rb.host`](/reference/host).
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

`rb.package()` declares packages that are resolved into `rootbeer.lock` before
apply. Prefer explicit resolver prefixes, such as `aqua:`, when names could
overlap between package backends.

```lua
rb.package("aqua:BurntSushi/ripgrep@14.1.1")
```

See the [Packages guide](/guide/packages) for lock/update/offline behavior.

## API Reference

<!--@include: ../api/_generated/core.md-->
