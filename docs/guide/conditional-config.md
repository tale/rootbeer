# Conditional Config

Since your rootbeer config is just Lua, you don't need a templating language
for conditionals. Use `if` statements, build tables dynamically, and call
module functions when you're ready.

## System detection

`rb.data()` returns information about the current machine:

```lua
local rb = require("@rootbeer")
local sys = rb.data()

sys.os        -- "macos", "linux", etc.
sys.arch      -- "aarch64", "x86_64", etc.
sys.hostname  -- machine hostname
sys.home      -- home directory path
sys.username  -- current user
```

## Branching on OS

```lua
local rb = require("@rootbeer")
local zsh = require("@rootbeer/zsh")
local sys = rb.data()

local cfg = {
    path = "~/.zshrc",
    keybind_mode = "emacs",
    env = { EDITOR = "nvim" },
    aliases = { g = "git" },
    history = {},
    evals = { "mise activate zsh" },
}

if sys.os == "macos" then
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
local sys = rb.data()

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

if sys.hostname == "work-macbook" then
    cfg.user.email = "aarnav@company.com"
end

git.config(cfg)
```

## Shared helpers

For logic reused across modules, just write Lua functions:

```lua
local rb = require("@rootbeer")
local sys = rb.data()

local function is_mac()
    return sys.os == "macos"
end

local function is_work()
    return sys.hostname == "work-macbook"
end

-- Use anywhere
if is_mac() then
    rb.link_file("config/aerospace.toml", "~/.config/aerospace/aerospace.toml")
end
```

## The pattern

There's no special API for conditionals. The approach is always:

1. Build your config table with the common defaults
2. Mutate it with `if` branches for machine-specific overrides
3. Pass the final table to the module

This is the same model as Nix/home-manager — the config IS code — but with
Lua instead of Nix expressions.
