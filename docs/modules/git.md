# git

The git module manages your global Git config from Lua. It writes
`~/.gitconfig`, can manage a global gitignore, and gives common settings like
signing and Git LFS first-class fields.

```lua
local git = require("rootbeer.git")
```

## Configure Git

Most config maps directly to familiar Git settings. Use `extra` for
tool-specific sections that Rootbeer doesn't model directly, such as `delta`.

```lua
git.config({
    user = {
        name = "Aarnav Tale",
        email = "aarnav@tale.me",
    },
    editor = "nvim",
    pager = "delta",
    signing = { key = "ssh-ed25519 AAAA..." },
    lfs = true,
    pull_rebase = true,
    ignores = { ".DS_Store", "._*", "*~" },
    extra = {
        delta = { features = "color-only" },
    },
})
```

For different emails, signing keys, or ignores per machine, use
[Profiles](/guide/profiles).

## API Reference

<!--@include: ../api/_generated/git.md-->
