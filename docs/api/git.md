---
outline: deep
---

# git

Declarative git configuration. Manages your `~/.gitconfig` and global gitignore
from a single Lua table — no more hand-editing INI sections. Shortcuts like
`signing` and `lfs` wire up multiple gitconfig sections at once, and `extra`
lets you pass through tool-specific sections like `delta` or `interactive`.

```lua
local git = require("@rootbeer/git")
```

## Examples

### Basic

```lua
git.config({
    path = "~/.gitconfig",
    user = {
        name = "Aarnav Tale",
        email = "aarnav@tale.me",
    },
    editor = "nvim",
    pager = "delta",
    pull_rebase = true,
})
```

### Signing, LFS, and ignores

Setting `signing` automatically wires up `user.signingkey`, `gpg.format`,
`commit.gpgSign`, and `tag.gpgSign`. Setting `lfs = true` emits the full
`[filter "lfs"]` section. The `ignores` list writes a gitignore file next to
your gitconfig and points `core.excludesfile` at it.

```lua
git.config({
    path = "~/.gitconfig",
    user = {
        name = "Aarnav Tale",
        email = "aarnav@tale.me",
    },
    editor = "nvim",
    pager = "delta",
    signing = {
        key = "ssh-ed25519 AAAA...",
    },
    lfs = true,
    pull_rebase = true,
    merge_conflictstyle = "diff3",
    ignores = {
        ".DS_Store",
        "._*",
        ".Trash-*",
        "*~",
    },
    extra = {
        delta = {
            features = "color-only",
            ["zero-style"] = "dim syntax",
        },
        interactive = {
            diffFilter = "delta --color-only",
        },
    },
})
```

## API Reference

<!--@include: ./_generated/git.md-->
