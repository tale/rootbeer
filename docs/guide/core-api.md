# Core API

The core API is available via `require("rootbeer")` and provides the
primitives for writing files, creating symlinks, and querying system info.

```lua
local rb = require("rootbeer")
```

## Writing Files

Write content to any path. Parent directories are created automatically,
and `~` expands to `$HOME`.

```lua
rb.file("~/.config/starship.toml", [[
[character]
success_symbol = "[➜](bold green)"
]])
```

<!--@include: ../_generated/core.file.md-->

## Symlinking

Symlink files from your source directory into place. The source path is
relative to `~/.local/share/rootbeer/source/`. Idempotent — existing
correct links are skipped, stale links are replaced.

```lua
rb.link_file("config/gitconfig", "~/.gitconfig")
rb.link_file("config/nvim", "~/.config/nvim")
```

<!--@include: ../_generated/core.link_file.md-->

## System Info

Query the current machine to write conditional configs that work across
multiple systems.

```lua
local d = rb.data()

if d.os == "Darwin" then
    rb.file("~/.config/homebrew/env", 'export HOMEBREW_PREFIX="/opt/homebrew"\n')
end
```

<!--@include: ../_generated/core.data.md-->

## JSON Serialization

Generate JSON config files for tools like VS Code or Alacritty.

```lua
rb.file("~/.config/alacritty/alacritty.json", rb.to_json({
    terminal = { shell = "/bin/zsh" },
    font = { size = 14 },
}))
```

<!--@include: ../_generated/core.to_json.md-->

## Low-Level Primitives

These are building blocks used internally by modules. Prefer `rb.file()`
with module renderers for most use cases.

<!--@include: ../_generated/core.line.md-->

<!--@include: ../_generated/core.emit.md-->

## Utilities

<!--@include: ../_generated/core.interpolate_table.md-->

<!--@include: ../_generated/core.register_module.md-->
