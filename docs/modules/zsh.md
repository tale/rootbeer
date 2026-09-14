# zsh

The zsh module lets you describe your shell setup as one Lua table. Rootbeer
turns it into the files zsh expects: environment, login profile, interactive
settings, aliases, history, completions, and startup commands.

```lua
local zsh = require("rootbeer.zsh")
```

## Configure zsh

Start with the settings you would normally spread across `.zshenv`,
`.zprofile`, and `.zshrc`:

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

Rootbeer manages your Zsh startup files through `~/.zshenv` and makes installed
packages available in login shells. Zsh must already be installed; after
`rb apply`, run `zsh -l` to start it with your settings.

For machine-specific values like work-only aliases or a different `EDITOR`, use
[Profiles](/guide/profiles).

## API Reference

<!--@include: ../api/_generated/zsh.md-->
