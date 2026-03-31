# Getting Started

Rootbeer is a dotfile manager that lets you define your system configuration
in Lua scripts. Think [chezmoi](https://www.chezmoi.io/), but with the full
power of a real scripting language instead of Go templates.

## Installation

Install rootbeer and bootstrap a dotfiles repo in one command:

```bash
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init tale/dotfiles
```

This installs the latest nightly `rb` to `~/.rootbeer/bin/`, clones your repo
into `~/.config/rootbeer/`, and you're ready to apply. Add `~/.rootbeer/bin`
to your `PATH` to use `rb` going forward.

To update to the latest nightly at any time:

```bash
rb update
```

To start fresh instead of cloning an existing repo:

```bash
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init
```

This creates `~/.config/rootbeer/` with a starter `init.lua` manifest. This
directory is your dotfiles repo — commit it, push it, clone it on another
machine.

### Private repos and SSH

The bootstrap clones over HTTPS by default. If your SSH keys are already
set up, use `--ssh`:

```bash
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init --ssh tale/dotfiles
```

You can also pass a full git URL directly:

```bash
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init git@github.com:tale/dotfiles.git
```

If you bootstrap over HTTPS and want to switch to SSH later, use
`rb remote ssh` or declare the remote in your config with `rb.remote()` —
see [Core Concepts](/guide/core-concepts#source-remote).

## Your First Config

Open your manifest with `rb edit`, or `rb cd` to jump into the source
directory:

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

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

## Editor Autocomplete

`rb init` automatically sets up [lua-language-server](https://luals.github.io/)
so you get full autocomplete and type checking out of the box. If you need to
regenerate the setup (or set it up manually), run:

```bash
rb lsp
```

This extracts type definitions to `~/.local/share/rootbeer/typedefs/` and
writes a `.luarc.json` in your source directory. Any editor with
lua-language-server support (VS Code, Neovim, etc.) will pick up completions
for all `require("rootbeer.*")` modules automatically.

## Next Steps

- [Core Concepts](/guide/core-concepts) — Plan/execute model, file operations, conditionals
- [Multi-Device Config](/guide/multi-device) — Profiles and per-machine branching
- [Core API Reference](/reference/core) — Full reference for `rb.*` functions
