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
local zsh = require("@rootbeer/zsh")
```

## Examples

### Basic

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
        vim = "nvim",
    },
    history = {},
    evals = { "mise activate zsh" },
})
```

### Full featured

```lua
local rb = require("@rootbeer")

zsh.config({
    env = {
        EDITOR = "nvim",
        VISUAL = "$EDITOR",
        OS = "$(uname -s)",
        SSH_AUTH_SOCK = rb.host.home .. "/.config/1Password/agent.sock",
    },
    profile = {
        evals = { "/opt/homebrew/bin/brew shellenv" },
        sources = { "~/.orbstack/shell/init.zsh" },
        path_prepend = { "$HOME/.amp/bin" },
    },
    keybind_mode = "emacs",
    options = { "CORRECT", "EXTENDED_GLOB" },
    prompt = "%F{cyan}%~%f%F{red}${vcs_info_msg_0_}%f %F{white}>%f ",
    aliases = {
        g = "git",
        ga = "git add -p",
        gc = "git commit",
        gd = "git diff",
        gs = "git status",
        ls = "lsd -l --group-directories-first",
    },
    history = {
        size = 10000,
        ignore = "ls*",
    },
    completions = {
        vi_nav = true,
        cache = "${XDG_CACHE_HOME:-$HOME/.cache}/zsh/.zcompcache",
        styles = {
            [":completion:*:*:*:*:descriptions"] = "format '%F{green}--> %d%f'",
            [":completion:*:*:*:*:corrections"] = "format '%F{yellow}!-> %d (errors: %e)%f'",
            [":completion:*:warnings"] = "format '%F{red}!-> no matches found%f'",
        },
    },
    functions = {
        plsdns = "sudo dscacheutil -flushcache\nsudo killall -HUP mDNSResponder",
    },
    evals = { "mise activate zsh" },
    sources = { "$ZDOTDIR/completions.zsh" },
})
```

## API Reference

<!--@include: ./_generated/zsh.md-->
