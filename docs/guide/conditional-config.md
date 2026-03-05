# Conditional Config

Since your rootbeer config is just Lua, you don't need a templating language
for conditionals. Use `if` statements, build tables dynamically, and call
module functions when you're ready.

## System detection

`rb.host` is a table with information about the current machine and user:

```lua
local rb = require("@rootbeer")

rb.host.os       -- "macos", "linux"
rb.host.arch     -- "aarch64", "x86_64"
rb.host.hostname -- machine hostname (or nil)
rb.host.distro   -- "ubuntu", "arch" (nil on macOS)
rb.host.user     -- current username
rb.host.home     -- home directory path
rb.host.shell    -- default login shell
```

See the full [`rb.host` reference](/api/host) for details.

## Branching on OS

```lua
local rb = require("@rootbeer")
local zsh = require("@rootbeer/zsh")

local cfg = {
    path = "~/.zshrc",
    keybind_mode = "emacs",
    env = { EDITOR = "nvim" },
    aliases = { g = "git" },
    history = {},
    evals = { "mise activate zsh" },
}

if rb.host.os == "macos" then
    cfg.path_prepend = { "$HOME/.amp/bin" }
    cfg.evals[#cfg.evals + 1] = "/opt/homebrew/bin/brew shellenv"
    cfg.functions = {
        plsdns = 'sudo dscacheutil -flushcache\nsudo killall -HUP mDNSResponder',
    }
end

zsh.config(cfg)
```

## Branching on hostname

Useful for work vs personal machines:

```lua
local rb = require("@rootbeer")
local git = require("@rootbeer/git")

local cfg = {
    path = "~/.gitconfig",
    user = {
        name = "Aarnav Tale",
        email = "aarnav@personal.me",
    },
    editor = "nvim",
    signing = { key = "ssh-ed25519 AAAA..." },
    pull_rebase = true,
}

if rb.host.hostname == "work-macbook" then
    cfg.user.email = "aarnav@company.com"
end

git.config(cfg)
```

## Shared helpers

For logic reused across modules, just write Lua functions:

```lua
local rb = require("@rootbeer")

local function is_mac()
    return rb.host.os == "macos"
end

local function is_work()
    return rb.host.hostname == "work-macbook"
end

-- Use anywhere
if is_mac() then
    rb.link_file("config/aerospace.toml", "~/.config/aerospace/aerospace.toml")
end
```

## Profiles

For full per-machine configurations, use [profiles](/api/profile). Define all
your machines in the repo and select at the command line — the same model as
NixOS flakes:

```lua
local profile = require("@rootbeer/profile")

profile.config({
    work = "hosts/work.lua",
    personal = "hosts/personal.lua",
})
```

```bash
rb apply work       # runs hosts/work.lua
rb apply personal   # runs hosts/personal.lua
```

## The pattern

There's no special API for conditionals. The approach is always:

1. Build your config table with the common defaults
2. Mutate it with `if` branches for machine-specific overrides
3. Pass the final table to the module

This is the same model as Nix/home-manager — the config IS code — but with
Lua instead of Nix expressions.
