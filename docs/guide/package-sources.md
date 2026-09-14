# Other package sources

[Browse the catalog](/packages/) for packages available by name. For another tool,
you can request a GitHub release or an Aqua recipe with `rb run`, `rb use`, or
`rb.packages()`.

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

## Use another index

A Lua configuration can select a custom collection before declaring packages:
call `rb.package_index()` with the snapshot URL and SHA-256 supplied by its
publisher. See the [API reference](/reference/core#rootbeer-package-index) for the
fields. HTTPS and absolute local `file://` snapshot URLs are supported.

Use a publisher you trust: the checksum identifies exact contents but does not
verify who published them. Rootbeer uses this collection instead of its official
catalog for the configuration. Standalone `rb run` and `rb use` do not read this
Lua setting.

`rb apply --update` keeps the selected snapshot. Change its URL and checksum to
select a newer one; changing or removing the setting refreshes your lock on the
next apply. Offline installs still require a matching lock and cached packages.

See [updates and offline use](/guide/package-locks) for saved versions, or
[index hosting and trust](/contributing/package-hosting) for verification and
fallback behavior.
