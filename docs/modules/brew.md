# brew

Declarative Homebrew package management. Define your formulae, casks, and Mac
App Store apps as a Lua table — rootbeer generates a Brewfile and runs
`brew bundle` to apply it.

```lua
local brew = require("rootbeer.brew")
```

## Example

```lua
brew.config({
    formulae = { "git", "ripgrep", "fzf", "jq", "mise" },
    casks = { "ghostty", "raycast", "1password" },
    mas = {
        { name = "Things", id = 904280696 },
    },
})
```

For conditional packages per machine, see [Multi-Device Config](/guide/multi-device).

## API Reference

<!--@include: ../api/_generated/brew.md-->