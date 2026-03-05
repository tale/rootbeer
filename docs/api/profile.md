---
outline: deep
---

# profile

Per-machine configuration via profiles. Define all your machine configs in your
repo and select which one to apply at the command line — the same model as
NixOS flakes if you are familiar with that.

::: tip
Profiles are optional. If you don't need different configs for different
machines, you don't need this at all.
:::

```lua
local profile = require("@rootbeer/profile")
```

## Usage

```bash
rb apply work        # runs hosts/work.lua
rb apply personal    # runs hosts/personal.lua
rb apply             # no profile selected, config is skipped
```

```lua
-- rootbeer.lua
local profile = require("@rootbeer/profile")

profile.config({
    work = "hosts/work.lua",
    personal = "hosts/personal.lua",
})
```

Each host file is a regular Lua script — no function wrapping needed:

```lua
-- hosts/work.lua
local git = require("@rootbeer/git")

git.config({
    user = {
        name = "Aarnav Tale",
        email = "aarnav@company.com",
    },
})
```

All paths are validated before the selected profile runs. If any referenced
file is missing, you get an error immediately — even if that profile wasn't
selected.

The raw value is also available as [`rb.profile`](/api/core#rb-profile) for
use outside of this module.

## Using `rb.profile` directly

`profile.config` is a convenience helper, but you don't have to use it.
`rb.profile` is a regular string (or `nil`) that you can branch on however
you like:

```lua
local rb = require("@rootbeer")

if rb.profile == "work" then
    -- work-specific config
end

-- shared config that applies to all profiles
```

## API Reference

<!--@include: ./_generated/profile.md-->
