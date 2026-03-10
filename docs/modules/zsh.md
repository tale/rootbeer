---
outline: deep
---

# zsh

Declarative zsh configuration. Describe your entire shell setup — environment
variables, login profile, options, aliases, prompt, history, completions — as
a single Lua table and rootbeer generates all the zsh files for you:
`~/.zshenv` (bootstrap), `<dir>/.zshenv`, `<dir>/.zprofile`, and
`<dir>/.zshrc`.

```lua
local zsh = require("rootbeer.zsh")
```

## Example

```lua
zsh.config({
    keybind_mode = "emacs",
    options = { "CORRECT", "EXTENDED_GLOB" },
    env = {
        EDITOR = "nvim",
        VISUAL = "$EDITOR",
    },
    aliases = {
        g = "git",
        ls = "lsd -l --group-directories-first",
    },
    history = { size = 10000 },
    evals = { "mise activate zsh" },
})
```

For per-machine overrides, see [Multi-Device Config](/guide/multi-device).
For the conditionals pattern, see [Core Concepts](/guide/core-concepts#conditionals).

## API Reference

<!--@include: ../api/_generated/zsh.md-->