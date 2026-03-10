---
outline: deep
---

# Multi-Device Config

::: tip
If you only use rootbeer on a single machine, you can skip this page entirely.
:::

One config repo, multiple machines. Profiles are the recommended way to
manage this — define a named config per machine and select it at apply time.
For small inline tweaks, you can also branch on `rb.host` fields directly.

## Profiles

Profiles give each machine its own entrypoint. Define them in your manifest
and select one at the command line — the same model as NixOS flakes:

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

### Using `rb.profile` Directly

`profile.config()` is a convenience helper, but you can also branch on the
raw `rb.profile` string yourself:

```lua
local rb = require("rootbeer")

if rb.profile == "work" then
    -- work-specific config
end

-- shared config that applies to all profiles
```

## Host Detection

For differences too small to warrant separate files, branch inline using
`rb.host`:

```lua
local rb = require("rootbeer")
local host = rb.host
local git = require("rootbeer.git")

local cfg = {
    user = {
        name = "Aarnav Tale",
        email = "aarnav@personal.me",
    },
    editor = "nvim",
    signing = { key = "ssh-ed25519 AAAA..." },
    pull_rebase = true,
}

if host.hostname == "work-macbook" then
    cfg.user.email = "aarnav@company.com"
end

git.config(cfg)
```

See the [Host reference](/reference/host) for all available fields.

## API Reference

<!--@include: ../api/_generated/profile.md-->
