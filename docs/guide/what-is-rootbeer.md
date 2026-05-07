# What is Rootbeer?

At a high level, Rootbeer is a tool used to manage your system's configuration
using Lua scripts. It has a wide variety of use cases and supported concepts,
but at its core, Rootbeer is a souped-up dotfiles manager. It's similar in
spirit to tools like [chezmoi](https://chezmoi.io) and
[home-manager](https://github.com/nix-community/home-manager).

:::info
Rootbeer is still very much a work in progress and is adding new features. The
idea is to eventually go as deep as covering packaging and system services.
:::

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

### Plan, then Execute
Rootbeer is built around a two-phase model:

1. **Planning**: Evaluate your config, build a plan of the desired state,
   compare it to the current state of the system, and figure out what changes
   need to be made.

2. **Execution**: Using a list of planned changes, execute them in a single run.
   The idea is to be idempotent and only make changes when necessary. If you run
   `rb apply` twice in a row, the second run should be a no-op.

