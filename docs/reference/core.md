# Core API

Write files, create symlinks, run commands, and manage packages from your Lua config.

For system information, see [`rb.host`](/reference/host).
For secrets from 1Password, see [Secrets](/reference/secrets).
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

## Package declarations

`rb.packages({ "ripgrep", "jq" })` declares a package list.
`rb.package("ripgrep")` accepts one entry. Add `@version` to choose an exact version.
Rootbeer saves package versions in `rootbeer.lock`.

[Find packages](/packages/) or follow the [package guide](/guide/packages) to get started.

## API Reference

<!--@include: ../api/_generated/core.md-->
