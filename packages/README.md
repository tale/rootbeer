# Rootbeer package collection

Each Lua file defines one canonical package. Definitions are evaluated with no
globals or host I/O, validated, and embedded in `rb`. Adding a definition requires
no Rust changes. This initial collection imports release binaries through existing
backends; it does not build sources or publish artifacts yet.

```lua
return {
    name = "ripgrep",
    aliases = { "rg" },
    description = "Search file contents with regular expressions",
    homepage = "https://github.com/BurntSushi/ripgrep",
    default_version = "15.2.0",
    versions = {
        ["15.2.0"] = {
            revision = 1,
            source = "aqua:BurntSushi/ripgrep@15.2.0",
            systems = { "aarch64-macos", "x86_64-macos", "aarch64-linux", "x86_64-linux" },
            bins = { "rg" },
            checks = { { "rg", "--version" } },
        },
    },
}
```

The filename must match the canonical name. Names and aliases are unique across
the collection. Versions identify upstream releases; increment `revision` when
changing the recipe for an existing version. Keep older recipes when introducing
a new version, and deliberately select `default_version`.

Sources must use an explicit `aqua:` or `github:` backend and exact version/tag.
The backend determines the artifact for the target system. Rootbeer never tries
another backend when that source fails. Only the declared commands are exported;
the backend must supply all of them. Checks execute argument arrays directly,
without a shell.

Platform declarations are test targets, not a claim of support for every OS
release or Linux libc. The workflow currently checks Ubuntu 24.04 (glibc) and
macOS 15 on Intel and ARM. Musl and older OS baselines are not yet qualified.

```sh
cargo build --release --bin rb
target/release/rb package check
target/release/rb package list
target/release/rb package show rg
target/release/rb package index > /tmp/rootbeer-index.json
python3 scripts/check-packages.py --rb target/release/rb
```

The checks use temporary home/state directories, execute installed commands, and
recreate their store and profile offline from cached downloads and an unchanged
lockfile. They require network
access for initial resolution and downloads. `--package ripgrep` selects one
package. Catalog CI runs on relevant pushes to `main` and pull requests targeting
`main`, only in public repositories and using standard runners;
it has no publication permissions or persistent artifact uploads.

## Index and provenance

`rb package index` emits deterministic schema-1 JSON. `rb package check` prints
the SHA-256 of its compact JSON representation. That catalog digest, the recipe
revision, exact backend request, and backend provenance are recorded together in
each resolution proof. This is provenance, not a publisher signature.

A matching lock remains usable after upgrading `rb`. If a changed configuration
needs resolution against a different embedded catalog, use the matching binary
or explicitly refresh the resolver inputs with `rb apply --update`.

The initial catalog travels with `rb`; exported indexes are inspection/build
artifacts, not remotely consumed indexes. Keep this collection here until its
format is exercised before extracting a separately published repository.

## Remaining build and publication work

The next stages require source/build/runtime dependency declarations, controlled
build environments, and package portability rules. The seven CLI acceptance
targets are Git, curl, xz, make, rsync, wget, and a telnet implementation. They are
not supported by this collection yet.

Publishing will require signed immutable index snapshots, a client trust root,
and verified artifact retrieval (initially public GHCR). Publication must be a
separate reviewed workflow: untrusted package tests must not receive publishing
credentials. Existing binaries can be imported without rebuilding; source builds
must identify their full inputs and dependencies. No hosted endpoint or trust key
is assumed by this implementation.
