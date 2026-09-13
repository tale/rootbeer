# What is Rootbeer?

Rootbeer manages your tools, dotfiles, and shell settings from a Lua configuration.
Keep it in Git and use it to set up your macOS and Linux machines.

## Core Concepts

### Config is Code

Your Rootbeer config is Lua, that's it. You get the expressiveness of a full
programming language to build your system configuration. There's no special
syntax.

```lua
-- An example of one of our high-level modules for managing Zsh configs.
-- When running `rb apply`, this will generate the appropriate files
-- including `.zprofile` and `.zshrc` with the options.

local zsh = require("rootbeer.zsh")
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

### Packages Belong in Your Config

Add tools alongside your other settings:

```lua
local rb = require("rootbeer")

rb.package("ripgrep")
```

Rootbeer saves package versions in `rootbeer.lock` so you choose when to update.
Follow [the package guide](/guide/packages) to make installed commands available
in your shell.

### Plan, then Execute

Preview changes with `rb apply --dry-run`, then make them with `rb apply`.
Rootbeer compares your configuration with the current files and only changes
what is needed.
