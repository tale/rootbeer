---
outline: deep
---

# brew

Declarative Homebrew package management. Define your formulae, casks, and Mac
App Store apps as a Lua table — rootbeer generates a Brewfile and runs
`brew bundle` to apply it.

```lua
local brew = require("@rootbeer/brew")
```

## Examples

### Basic

```lua
brew.config({
    formulae = { "git", "ripgrep", "fzf", "jq" },
    casks = { "ghostty", "raycast" },
})
```

### Full featured

```lua
local rb = require("@rootbeer")

local casks = {
    "1password",
    "1password-cli",
    "brave-browser",
    "ghostty",
    "orbstack",
    "raycast",
    "slack",
    "spotify",
}

if rb.host.hostname ~= "work-macbook" then
    for _, c in ipairs({ "discord", "steam", "tailscale-app" }) do
        casks[#casks + 1] = c
    end
end

brew.config({
    formulae = {
        "curl",
        "fzf",
        "git",
        "git-delta",
        "git-lfs",
        "jq",
        "lsd",
        "mise",
        "ripgrep",
    },
    casks = casks,
    mas = {
        { name = "Streaks", id = 963034692 },
        { name = "Things", id = 904280696 },
    },
})
```

## API Reference

<!--@include: ./_generated/brew.md-->
