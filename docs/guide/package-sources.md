# Other package sources

[Browse the catalog](https://search.rbpkg.com) for packages available by name. For another tool,
you can request a GitHub release or an Aqua recipe with `rb run`, `rb use`, or
`rb.packages()`.

## Define packages locally

Keep registry-format recipes beside your configuration:

```text
rootbeer.lua
packages/
  my-tool.lua
```

Load them before declaring packages:

```lua
local rb = require("rootbeer")

rb.package_catalog("packages")
rb.packages({ "my-tool", "jq" })
```

Each file returns a [schema-2 package recipe](/contributing/packaging#definition-api),
with its filename matching the package name. Use the same `inputs`, `build`,
`outputs`, and `versions` fields as registry recipes. GitHub and Aqua prebuilts,
source archives, Rust, Zig, Autotools, and custom build phases are supported.
A source recipe's archive URL and SHA-256 remain required.

The directory is relative to the configuration script; absolute paths and `~`
also work. Call `package_catalog` once. Local names and aliases take precedence
over the selected registry, while other requests still use the registry normally.
Explicit `github:` and `aqua:` requests keep their original meaning.

Recipes can depend on other local recipes or packages from the selected registry.
Source-capable local recipes build locally by default, even if they also declare
upstream prebuilts. Planning reads and validates recipes without downloading or
building. Apply runs the shared build executor and its checks, then installs the
result in the normal package profile.

Recipe contents are recorded in `rootbeer.lock`. Editing a local recipe refreshes
local package resolutions on the next apply; unchanged registry requests retain
their locks. `--locked` rejects recipe changes, and `--offline` replays matching
locks and cached packages. Use ordinary apply once after changing a recipe.
`--update` refreshes resolutions but does not discover and rewrite local versions;
maintain their `versions` and defaults in the recipe files.

The setting applies to this Lua configuration. Standalone `rb run` and `rb use`
do not load it. Recipes stay on your machine; no index publication is needed.

## Install from GitHub

Use `github:owner/repository@tag`:

```sh
rb run github:BurntSushi/ripgrep@15.2.0 -- --version
rb use github:BurntSushi/ripgrep@15.2.0
```

Or declare the same request in your configuration:

```lua
local rb = require("rootbeer")

rb.packages({ "github:BurntSushi/ripgrep@15.2.0" })
```

The release tag must match exactly, including a leading `v` when the project uses
one. Rootbeer selects a release asset for your platform. Supported formats include
tar.gz, tar.xz, ZIP, and standalone executables.

If automatic selection is ambiguous, put the request and its options in one entry.
Choose an exact release asset and name its exported command paths:

```lua
rb.packages({
    "jq",
    {
        request = "github:BurntSushi/ripgrep@15.2.0",
        asset = "ripgrep-15.2.0-aarch64-apple-darwin.tar.gz",
        bins = { rg = "ripgrep-15.2.0-aarch64-apple-darwin/rg" },
    },
})
```

This asset targets Apple silicon macOS; select platform-specific declarations with
normal Lua and `rb.host`. Keep versions in the request's `@version` syntax and
resolvers in its `github:` or `aqua:` prefix. `asset` and `bins` are options for
explicit GitHub requests. The CLI's `--bin` chooses an already exported command;
it does not select a release asset or an archive path.

`rb.package(entry)` accepts the same individual entry. Existing
`rb.package("github:owner/repository@tag", { asset = "...", bins = { ... } })`
calls remain supported. Exact raw package tables can also appear in the list;
see the [package API](/reference/core#rootbeer-packages).

## Install from Aqua

Use `aqua:owner/repository@version` to select an Aqua registry recipe. The prefix
works with all three installation methods; you do not need Aqua installed.

Direct GitHub and Aqua requests record download hashes in their saved resolutions.
They do not carry the Rootbeer index publisher's signature. Aqua signatures and
attestations are not verified.

## Use another repository

A Lua configuration can use another package repository in place of the official one:
call `rb.package_repository()` before declaring packages, with the URL of its signed
root and its publisher's verification key. See the
[API reference](/reference/core#rootbeer-package-repository) for the fields. HTTPS and
absolute local `file://` URLs are supported.

The key is what you trust: Rootbeer verifies the repository's signature, rejects
rollbacks, and checks every package document and record against the digests that
signature covers. Standalone `rb run` and `rb use` do not read this Lua setting.

Your lock records the exact root it resolved against. `rb apply --update` moves it to
the repository's latest root; changing or removing the setting refreshes your lock on
the next apply. Offline installs still require a matching lock and cached packages.

See [updates and offline use](/guide/package-locks) for saved versions, or
[repository hosting and trust](/contributing/package-hosting) for verification and
fallback behavior.
