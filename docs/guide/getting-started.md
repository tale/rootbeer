# Getting Started

Rootbeer is a dotfile manager that lets you define your system configuration
in Lua scripts. Think [chezmoi](https://www.chezmoi.io/), but with the full
power of a real scripting language instead of Go templates.

## Installation

Install rootbeer and bootstrap a dotfiles repo in one command:

```bash
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init tale/dotfiles
```

This installs `rb`, clones your repo into `~/.local/share/rootbeer/source/`,
and you're ready to apply.

To start fresh instead of cloning an existing repo:

```bash
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init
```

This creates `~/.local/share/rootbeer/source/` with a starter `rootbeer.lua`
manifest. This directory is your dotfiles repo — commit it, push it, clone
it on another machine.

## Your First Config

Open your manifest with `rb edit`, or `rb cd` to jump into the source
directory:

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

## Next Steps

- [Core Concepts](/guide/core-concepts) — Plan/execute model, file operations, conditionals
- [Multi-Device Config](/guide/multi-device) — Profiles and per-machine branching
- [Core API Reference](/reference/core) — Full reference for `rb.*` functions
