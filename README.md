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
local rb = require("rootbeer")
local zsh = require("rootbeer.shells.zsh")
local d = rb.data()

-- Write a generated .zshrc
rb.file("~/.zshrc", zsh.config {
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
})

-- Conditionals based on system data
if d.os == "Darwin" then
    rb.file("~/.config/homebrew/env", 'export HOMEBREW_PREFIX="/opt/homebrew"\n')
end

-- Symlink a file from your source directory
rb.link_file("config/gitconfig", "~/.gitconfig")
```

## Lua API

### Core (`require("rootbeer")`)

| Function | Description |
|---|---|
| `rb.file(path, content)` | Write `content` to `path` (`~` expands to `$HOME`) |
| `rb.link_file(src, dest)` | Symlink `src` (relative to source dir) to `dest` (`~` on target) |
| `rb.data()` | Returns `{os, arch, hostname, home, username}` |
| `rb.to_json(table)` | Serialize a Lua table to JSON |
| `rb.interpolate_table(tbl, fn)` | Pass `tbl` through `fn` and return the result |
| `rb.line(str)` | Append a line (with newline) to the output buffer |
| `rb.emit(str)` | Append raw text to the output buffer |
| `rb.register_module(name, tbl)` | Register a native module |

### Shells (`require("rootbeer.shells.zsh")`)

`zsh.config(table)` renders a declarative config table into a zshrc string.
Supported keys: `env`, `aliases`, `evals`, `sources`, `extra`.

## Building

Requires `meson` and `ninja`. LuaJIT and cJSON are vendored as subprojects.

```bash
meson setup build
meson compile -C build
```

The binary is built to `./build/src/rootbeer_cli/rb`.

If you have [mise](https://mise.jdx.dev/) installed:
```bash
mise run build
```

## License

MIT
