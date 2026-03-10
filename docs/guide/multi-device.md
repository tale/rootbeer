---
outline: deep
---

# Multi-Device Config

::: tip
If you only use rootbeer on a single machine, you can skip this page entirely.
:::

One config repo, multiple machines. Rootbeer gives you three tools for
profile-dependent config, from lightweight inline switches to full
per-machine files. Mix and match as needed.

## `profile.select` — Pick a Value

The most common case: one field differs between profiles. `select` returns
the value matching the active profile, falling back to `default` when the
profile isn't listed.

```lua
local profile = require("rootbeer.profile")
local git = require("rootbeer.git")

git.config({
    user = {
        name = "Aarnav Tale",
        email = profile.select({
            default = "aarnav@personal.me",
            work = "aarnav@company.com",
        }),
    },
    editor = "nvim",
})
```

Works anywhere a value is expected — strings, tables, lists:

```lua
local brew = require("rootbeer.brew")

local extra_casks = profile.select({
    personal = { "discord", "steam", "signal" },
    work = { "linear-linear", "notion", "slack" },
})
```

If the active profile has no entry and no `default` is set, `select` errors
with a clear message listing the known keys.

## `profile.when` — Conditional Blocks

For entire sections that only apply to certain profiles. Accepts a single
name or a list.

```lua
local profile = require("rootbeer.profile")
local mac = require("rootbeer.mac")

profile.when("personal", function()
    mac.hostname({ name = "Aarnavs-MBP" })
    mac.touch_id_sudo()
end)

profile.when({"work", "personal"}, function()
    -- runs for either profile
    mac.dock({ autohide = true })
end)
```

When `rb.profile` is `nil` (no profile specified), `when` is a no-op.

## `profile.config` — Separate Files

When differences are large enough that inline switches get unwieldy, split
into dedicated files. This is the same model as NixOS flakes — each profile
gets its own entrypoint.

```lua
-- rootbeer.lua
local profile = require("rootbeer.profile")

profile.config({
    work = "hosts/work.lua",
    personal = "hosts/personal.lua",
})
```

Each host file is a regular Lua script:

```lua
-- hosts/work.lua
local git = require("rootbeer.git")

git.config({
    user = {
        name = "Aarnav Tale",
        email = "aarnav@company.com",
    },
})
```

```bash
rb apply work        # runs hosts/work.lua
rb apply personal    # runs hosts/personal.lua
```

All paths are validated before the selected profile runs. If any referenced
file is missing, you get an error immediately — even if that profile wasn't
selected.

## Combining Approaches

A typical setup uses all three together — `select` for individual values,
`when` for conditional blocks, and `config` for large profile-specific
overrides at the end:

```lua
local rb = require("rootbeer")
local profile = require("rootbeer.profile")
local git = require("rootbeer.git")
local brew = require("rootbeer.brew")
local mac = require("rootbeer.mac")

-- Shared config with inline value switches
git.config({
    user = {
        name = "Aarnav Tale",
        email = profile.select({
            default = "aarnav@personal.me",
            work = "aarnav@company.com",
        }),
    },
})

-- Entire block only for personal machines
profile.when("personal", function()
    mac.touch_id_sudo()
    mac.hostname({ name = "Aarnavs-MBP" })
end)

-- Heavy per-machine overrides in separate files
profile.config({
    work = "hosts/work.lua",
    personal = "hosts/personal.lua",
})
```

## Host Detection

For differences that don't map to profiles (OS, architecture), branch on
`rb.host`:

```lua
local rb = require("rootbeer")

if rb.host.os == "macos" then
    -- macOS-only config
end
```

See the [Host reference](/reference/host) for all available fields.

## API Reference

<!--@include: ../api/_generated/profile.md-->
