# brew

The brew module manages Homebrew packages from Lua. Declare your formulae,
casks, and Mac App Store apps; Rootbeer generates a Brewfile and applies it with
`brew bundle`.

```lua
local brew = require("rootbeer.brew")
```

## Configure Homebrew

```lua
brew.config({
    formulae = { "git", "ripgrep", "fzf", "jq", "mise" },
    casks = { "ghostty", "raycast", "1password" },
    mas = {
        { name = "Things", id = 904280696 },
    },
})
```

For packages that only belong on some machines, use [Profiles](/guide/profiles).

## API Reference

<!--@include: ../api/_generated/brew.md-->
