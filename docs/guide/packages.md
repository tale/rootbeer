# Packages

Rootbeer can install small, self-contained tools into its own package profile.
Declare the tools you want in Lua, commit the generated `rootbeer.lock`, and
source the profile from your shell.

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

rb.package("aqua:BurntSushi/ripgrep@14.1.1")

local package_env = rb.env_export("sh")
zsh.config({
    sources = { package_env },
})
```

Package resolver support is currently Aqua-backed. Prefer explicit versions when
you know what you want; unversioned requests follow the locked resolver input
until you update the lock.

## Apply and Commit

Run `rb apply` as usual:

```sh
rb apply
```

When packages are present, Rootbeer creates or reuses `rootbeer.lock` beside your
config. Commit it with the rest of your dotfiles. On another machine, the lock is
what keeps package selection stable.

For CI, bootstrap scripts, or any run that must not rewrite the lock, use:

```sh
rb apply --locked
```

This fails if `rootbeer.lock` is missing or no longer matches your Lua config.

## Update Deliberately

Normal `rb apply` reuses a matching lock. To intentionally refresh resolver
inputs and rewrite package selections, run:

```sh
rb apply --update
```

Review and commit the resulting `rootbeer.lock` change just like you would review
a dependency lockfile update.

## Work Offline

Use offline mode when the lock and package artifacts should already be present:

```sh
rb apply --offline
```

Offline mode will not fetch missing sources. It can reuse an existing store
output or a source archive already in Rootbeer's download cache.

## What the Lock Protects

`rootbeer.lock` records the package decision Rootbeer made: the resolver input,
selected source, source hash, provided binaries, and realized output hash. For
snapshot-style resolvers like Aqua, it also records the exact registry revision
used for resolution.

Resolvers do not all have to look like Aqua. If a resolver cannot pin one whole
registry revision, Rootbeer records the metadata it used instead. A locked apply
should make the same decision or fail, never drift silently.

Prefer explicit resolver prefixes when a name could exist in more than one
backend:

```lua
rb.package("aqua:cli/cli@v2.47.0")
```
