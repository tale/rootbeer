# Getting Started

Rootbeer is a dotfile manager that lets you define your system configuration
in Lua scripts. Think [chezmoi](https://www.chezmoi.io/), but with the full
power of a real scripting language instead of Go templates.

## Installation

Rootbeer is built from source using Cargo.

```bash
cargo build --release
```

If you have [mise](https://mise.jdx.dev/) installed:

```bash
mise run build
```

## Initializing

Set up a new source directory, or clone an existing dotfiles repo:

```bash
# Create a fresh source directory with a starter manifest
rb init

# Or clone from a GitHub repo
rb init tale/dotfiles
```

This creates `~/.local/share/rootbeer/source/` with a `rootbeer.lua` manifest
file. This directory is your dotfiles repo — commit it, push it, clone it on
another machine.

## Your First Config

Open `~/.local/share/rootbeer/source/rootbeer.lua`:

```lua
local rb = require("@rootbeer")
local zsh = require("@rootbeer/zsh")

rb.file("~/.zshrc", zsh.config({
    env = {
        EDITOR = "nvim",
        LANG = "en_US.UTF-8",
    },
    aliases = {
        g = "git",
        v = "nvim",
    },
    extra = "autoload -Uz compinit && compinit",
}))
```

This writes a generated `.zshrc` to your home directory.

## Applying

```bash
# Preview what would change
rb apply -n

# Apply for real
rb apply
```

`rb apply` evaluates your manifest and writes/links files into place. Use
`-n` (dry run) to see what would happen without touching the filesystem.

## Conditionals

Use `rb.host` to branch on the current machine:

```lua
local rb = require("@rootbeer")

if rb.host.os == "macos" then
    rb.file("~/.config/homebrew/env", 'export HOMEBREW_PREFIX="/opt/homebrew"\n')
end

if rb.host.hostname == "workstation" then
    -- work-specific config
end
```

See the full [`rb.host` reference](/api/host) for all available fields.

## Symlinking

For files you want to edit directly (like a gitconfig), use `rb.link_file()`
to symlink them from your source directory:

```lua
-- Source path is relative to the source directory
-- Target path supports ~ expansion
rb.link_file("config/gitconfig", "~/.gitconfig")
rb.link_file("config/nvim", "~/.config/nvim")
```

Symlinks are idempotent — if the link already points to the right place,
nothing happens. Stale links are replaced.

## Next Steps

- [Conditional Config](/guide/conditional-config) — Per-machine branching with `rb.host`
- [Core API](/api/core) — Full reference for `rb.*` functions
- [zsh](/api/zsh) — Declarative zsh configuration
- [git](/api/git) — Declarative git configuration
