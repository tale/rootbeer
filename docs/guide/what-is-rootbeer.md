# What is Rootbeer?

Rootbeer manages packages and system configuration through declarative Lua.
Describe the tools, files, shell settings, and machine-specific choices you want,
then apply them together from one configuration repository.

Packages are part of that desired state. The signed catalog supplies verified
binaries for supported platforms, while `rootbeer.lock` records exact selections.
The CLI runs standalone on macOS and Linux.

Rootbeer is still developing broader source-build and runtime dependency support.
See [package scope](/guide/package-sources#scope) for current boundaries.

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

Use [package search](/packages/) to choose canonical names and check platform
availability. Package declarations live alongside file and shell configuration;
the generated environment exposes their commands. Matching locks remain stable
until you explicitly update them.

The catalog and CLI have independent releases. New recipes can become available
without requiring an engine upgrade. Read [the package guide](/guide/packages)
for installation, shell integration, and lockfile behavior.

### Plan, then Execute

Rootbeer is built around a two-phase model:

1. **Planning**: Evaluate your config, build a plan of the desired state,
   compare it to the current state of the system, and figure out what changes
   need to be made.

2. **Execution**: Using a list of planned changes, execute them in a single run.
   The idea is to be idempotent and only make changes when necessary. If you run
   `rb apply` twice in a row, the second run should be a no-op.
