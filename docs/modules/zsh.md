# Zsh

The `rootbeer.shells.zsh` module provides a declarative way to generate
zsh configuration from a Lua table.

## Example

```lua
local zsh = require("rootbeer.shells.zsh")

rb.file("~/.zshrc", zsh.config({
  env = {
    EDITOR = "nvim",
    PATH = "$HOME/.local/bin:$PATH",
  },
  aliases = {
    ll = "ls -la",
    gs = "git status",
  },
  evals = {
    "mise activate zsh",
  },
  sources = {
    "$HOME/.config/zsh/plugins.zsh",
  },
  extra = "bindkey -v",
}))
```

## API Reference

<!--@include: ../_generated/shells.zsh.config.md-->
