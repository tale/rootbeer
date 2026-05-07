# git

Declarative git configuration. Manages your `~/.gitconfig` and global gitignore
from a single Lua table — no more hand-editing INI sections. Shortcuts like
`signing` and `lfs` wire up multiple gitconfig sections at once, and `extra`
lets you pass through tool-specific sections like `delta` or `interactive`.

```lua
local git = require("rootbeer.git")
```

## Example

`signing` wires up `user.signingkey`, `gpg.format`, `commit.gpgSign`, and
`tag.gpgSign` automatically. `lfs = true` emits the full `[filter "lfs"]`
section. `extra` passes through arbitrary gitconfig sections.

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

For per-machine overrides, see [Profiles](/guide/profiles).

## API Reference

<!--@include: ../api/_generated/git.md-->