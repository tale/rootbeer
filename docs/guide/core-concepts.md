# Core Concepts

## Config is Code

Your rootbeer config is Lua — not templates, not YAML. Use `if` statements,
loops, functions, and modules to build your system configuration.

```lua
local rb = require("rootbeer")

for _, dir in ipairs({ "projects", "scratch", "notes" }) do
    rb.file("~/" .. dir .. "/.keep", "")
end
```

## Plan & Apply

Calls like `rb.file()` and `rb.link_file()` don't write to disk immediately.
They queue operations into a plan. Running `rb apply` evaluates your manifest
and executes every queued operation. Use `rb apply -n` to preview the plan
without touching the filesystem.

## File Operations

### Writing Files

`rb.file()` writes generated content to a path:

```lua
local rb = require("rootbeer")

rb.file("~/.config/homebrew/env", 'export HOMEBREW_PREFIX="/opt/homebrew"\n')
```

### Symlinking

`rb.link_file()` symlinks a file from your source directory to a target path:

```lua
-- Source path is relative to the source directory
-- Target path supports ~ expansion
rb.link_file("config/gitconfig", "~/.gitconfig")
rb.link_file("config/nvim", "~/.config/nvim")
```

Symlinks are idempotent — if the link already points to the right place,
nothing happens. Stale links are replaced.

## Source Remote

`rb.remote()` declares the desired `origin` URL for the rootbeer source
directory. This is useful when you bootstrap over HTTPS on a fresh machine
and want to switch to SSH once keys are in place:

```lua
rb.remote("git@github.com:tale/dotfiles.git")
```

The change is idempotent — if the remote already matches, nothing happens.
You can also switch remotes manually at any time with the CLI:

```bash
rb remote ssh    # rewrite GitHub origin to SSH
rb remote https  # rewrite GitHub origin to HTTPS
rb remote        # print current origin URL
```

## Conditionals

There's no special API for conditionals — your config is just Lua. The
pattern is: build a config table, mutate it with `if` branches, pass it
to the module.

```lua
local rb = require("rootbeer")
local host = rb.host

local cfg = {
    env = { EDITOR = "nvim" },
    aliases = { g = "git" },
    evals = { "mise activate zsh" },
}

if host.os == "macos" then
    table.insert(cfg.evals, "/opt/homebrew/bin/brew shellenv")
end
```

For per-machine branching and profiles, see
[Multi-Device Config](/guide/multi-device). For all available `rb.host`
fields, see the [Host reference](/reference/host).
