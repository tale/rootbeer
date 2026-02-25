# Rootbeer
> Manage your dotfiles with Lua!

Rootbeer is a dotfile manager that lets you define your system configuration
in Lua scripts. Think [chezmoi](https://www.chezmoi.io/), but with the full
power of a real scripting language. Write declarative config tables, use
conditionals based on your OS/hostname, and generate shell configs — all
without learning a templating DSL.

## Quick Start
```bash
# Initialize with a new source directory
rb init

# Or clone an existing dotfiles repo
rb init tale/dotfiles

# Apply your configuration
rb apply

# Dry run (preview without writing files)
rb apply -n
```

`rb init` creates a source directory at `~/.local/share/rootbeer/source/`
with a starter `rootbeer.lua` manifest. When you run `rb apply`, it
evaluates the manifest and writes/links your dotfiles into place.

## Example Config

```lua
local rb = require("@rootbeer")
local zsh = require("@rootbeer/shells/zsh")
local d = rb.data()

-- Write a generated .zshrc
rb.file("~/.zshrc", zsh.config({
    env = {
        EDITOR = "nvim",
        LANG = "en_US.UTF-8",
    },
    aliases = {
        g = "git",
        v = "nvim",
        ls = "ls -la",
    },
    sources = { "~/.config/zsh/local.zsh" },
    extra = "autoload -Uz compinit && compinit",
}))

-- Conditionals based on system data
if d.os == "macos" then
    rb.file("~/.config/homebrew/env", 'export HOMEBREW_PREFIX="/opt/homebrew"\n')
end

-- Symlink a file from your source directory
rb.link_file("config/gitconfig", "~/.gitconfig")
```

## Lua API

### Core (`require("@rootbeer")`)

| Function | Description |
|---|---|
| `rb.file(path, content)` | Write `content` to `path` (`~` expands to `$HOME`) |
| `rb.link_file(src, dest)` | Symlink `src` (relative to source dir) to `dest` |
| `rb.data()` | Returns `{os, arch, hostname, home, username}` |

### Shells (`require("@rootbeer/shells/zsh")`)

`zsh.config(table)` renders a declarative config table into a zshrc string.
Supported keys: `env`, `aliases`, `evals`, `sources`, `extra`.

## How It Works

Rootbeer uses a **plan-first architecture**: your Lua script declares intent
(write this file, create this symlink) without touching the filesystem.
After the script completes, the plan is validated and executed — or in
dry-run mode, just printed.

## Building

Requires Rust 1.79+.

### With Cargo

```bash
cargo build
```

Builds the binary to `./target/debug/rb`.

### With Nix

```bash
nix build
```

Builds the binary to `./result/bin/rb`.

## License

MIT
