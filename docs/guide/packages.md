# Packages

Rootbeer can install small, self-contained tools into its own package profile.
Declare the tools you want in Lua, commit the generated `rootbeer.lock`, and
source the profile from your shell.

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

rb.package("ripgrep")

local package_env = rb.env_export("sh")
zsh.config({
    sources = { package_env },
})
```

Unqualified names use Rootbeer's canonical catalog. Each package has an approved
default version, declared commands, and explicit platform support. `rg` is an
alias for `ripgrep`; aliases retain the same canonical package identity.

```sh
rb package list
rb package show ripgrep
rb package check
```

The binary catalog contains `age`, `fd`, and `ripgrep`. XZ has an explicit source
build recipe; `rb package list` distinguishes `binary` and `source` entries.
The catalog ships inside `rb` and
does not require a registry service. Requests for unknown names or versions fail;
Rootbeer does not silently try another provider. Use `rb.package("ripgrep@15.2.0")`
to select an approved version explicitly.

To build XZ locally using the host compiler and Make:

```sh
rb package build xz --output /tmp/rootbeer-xz
rb apply --script /tmp/rootbeer-xz/install.lua
```

This verifies the source hash, runs the upstream build and test steps, and creates
an installable archive and build receipt. The output directory must be new. Builds
execute trusted source code without OS sandboxing. Normal `rb.package("xz")`
installation remains unavailable until binary publication is implemented.

Rootbeer also supports `aqua:` registry recipes and `github:` release assets without
requiring mise or Aqua to be installed. Prefer explicit versions when you know
what you want. Unversioned Aqua requests use the pinned registry's package index;
unversioned GitHub requests select the latest published stable release. Both
reuse `rootbeer.lock` until you update it.

## Choose a Backend

Use Aqua when the registry has a recipe. Rootbeer uses mise's `aqua-registry`
library for templates, version constraints, and platform overrides:

```lua
rb.package("aqua:junegunn/fzf")
rb.package("aqua:cli/cli")
rb.package("aqua:neovim/neovim")
rb.package("aqua:1password/cli")
```

Use GitHub for projects publishing binaries directly. Explicit versions are exact
release tags, including a leading `v` when the upstream tag uses one:

```lua
rb.package("github:BurntSushi/ripgrep@15.2.0")
```

Rootbeer selects assets matching the OS and architecture. Ambiguous matches
fail instead of guessing (for example, a Linux release with both glibc and musl
builds). Supply an exact filename and, optionally, the binaries to expose:

```lua
rb.package("github:BurntSushi/ripgrep@15.2.0", {
    asset = "ripgrep-15.2.0-aarch64-apple-darwin.tar.gz",
    bins = { rg = "ripgrep-15.2.0-aarch64-apple-darwin/rg" },
})
```

Without `bins`, GitHub archives expose regular executable files under their
original filenames; duplicate names fail. Raw assets use the repository name as
the command name. Overrides are part of the locked request.

Both backends support tar.gz, tar.xz, ZIP, and raw executable downloads. Archive
contents stay together in the store so accompanying runtime files are preserved.
ZIP symlinks and Aqua recipes requiring builds, custom installation, or Rosetta
are currently unsupported.

## Replacing Homebrew

These backends cover tools with self-contained upstream binaries, such as
chezmoi, fzf, gh, delta, git-lfs, jq, lsd, mise, mkcert, Neovim, rage, ripgrep,
and the 1Password CLI. Declare them with `rb.package`, then source
`rb.env_export("sh")` from your shell configuration as shown above.

Aqua and GitHub release downloads do not replace Homebrew's dependency solving,
services, cask installers, or Mac App Store handling. Formulae that require
source builds or shared libraries need another installation strategy. Keep
those entries in [the brew module](/modules/brew) until an alternative exists.
Declaring a package in Rootbeer does not uninstall its Homebrew copy.

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

Catalog resolutions additionally record the catalog digest, canonical identity,
recipe revision, and underlying backend proof. A matching lock remains usable
after upgrading `rb`. Resolving new requests against a different catalog requires
the matching binary or an explicit `--update`.

Use an explicit resolver prefix to bypass the canonical catalog:

```lua
rb.package("aqua:cli/cli@v2.47.0")
```

## Verification

Every downloaded artifact is hashed when creating the lock, and subsequent
installs verify that hash. The GitHub backend also checks an asset's SHA-256
digest when GitHub supplies one. Aqua signature, attestation, and checksum-file
verification are not yet implemented; hashing the first download records its
contents, but does not independently authenticate that initial download.

License notices for the package backend libraries are available with `rb licenses`.
