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

## Verifying your own signatures

Setting `signing.key` makes Git *create* signatures, but Git can't *verify*
them without an allowed signers file — `git log --show-signature` reports
`No signature` on your own commits. List the principals your key signs for and
Rootbeer writes the file and wires `gpg.ssh.allowedSignersFile` to it:

```lua
signing = {
    key = "ssh-ed25519 AAAA...",
    allowed_signers = { "aarnav@tale.me", "aarnav@work.example" },
},
```

Listing every identity you commit under, rather than just this machine's, means
commits made on one machine still verify on the others. This applies to SSH
signing only; it is skipped for other `format` values.

## Extending built-in sections

`extra` merges into the sections the shortcuts above already wrote, so you can
add to `[core]` or `[tag]` without losing what Rootbeer put there:

```lua
git.config({
    user = { name = "Aarnav Tale", email = "aarnav@tale.me" },
    editor = "nvim",
    ignores = { ".DS_Store" },
    extra = {
        -- Keeps core.editor and core.excludesfile.
        core = { fsmonitor = true, untrackedCache = true },
        rerere = { enabled = true, autoUpdate = true },
        fetch = { prune = true, pruneTags = true },
    },
})
```

On a key conflict `extra` wins, which makes it the escape hatch when a shortcut
picks a value you disagree with.

## API Reference

<!--@include: ../api/_generated/git.md-->
