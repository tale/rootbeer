---
outline: deep
---

# zsh

Declarative zsh configuration. Describe your shell setup — options, aliases,
prompt, history, completions — as a Lua table and let rootbeer render the
zshrc for you. Since the config is just Lua, you can branch on OS, hostname,
or anything else without a templating language.

```lua
local zsh = require("@rootbeer/zsh")
```

## Examples

### Basic

```lua
zsh.config({
    path = "~/.zshrc",
    keybind_mode = "emacs",
    options = { "CORRECT", "EXTENDED_GLOB" },
    env = {
        EDITOR = "nvim",
        LANG = "en_US.UTF-8",
    },
    aliases = {
        g = "git",
        ls = "lsd -l --group-directories-first",
        la = "lsd -la --group-directories-first",
        vim = "nvim",
    },
    history = {},
    evals = {
        "mise activate zsh",
    },
})
```

### Full featured

```lua
zsh.config({
    path = "~/.zshrc",
    keybind_mode = "emacs",
    options = { "CORRECT", "EXTENDED_GLOB" },
    env = {
        EDITOR = "nvim",
        VISUAL = "nvim",
    },
    path_prepend = { "$HOME/.amp/bin" },
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
        size = 50000,
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
        plsdns = 'sudo dscacheutil -flushcache\nsudo killall -HUP mDNSResponder',
    },
    evals = {
        "mise activate zsh",
    },
    sources = {
        "$ZDOTDIR/completions.zsh",
    },
})
```

## API Reference

<!--@include: ./_generated/zsh.md-->
