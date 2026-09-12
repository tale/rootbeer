# Rootbeer package collection

Each Lua file defines one canonical package. Definitions are evaluated with no
globals or host I/O, validated, and embedded in `rb`. Adding a definition requires
no Rust changes. The collection supports upstream release binaries and explicit
Autotools source builds. It does not publish artifacts yet.

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

Binary sources must use an explicit `aqua:` or `github:` backend and exact version/tag.
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

Source builds are described below. Remaining work includes runtime dependency
closures, pinned toolchains, OS sandboxing, and package portability rules. XZ is
the first source-build acceptance target; Git, curl, make, rsync, wget, and a
telnet implementation still need recipes and their dependencies.

Publishing will require signed immutable index snapshots, a client trust root,
and verified artifact retrieval (initially public GHCR). Publication must be a
separate reviewed workflow: untrusted package tests must not receive publishing
credentials. Existing binaries can be imported without rebuilding; source builds
must identify their full inputs and dependencies. No hosted endpoint or trust key
is assumed by this implementation.

## Source builds

Use `build` instead of `source` for a verified upstream source archive. See
[`xz.lua`](xz.lua) for a working recipe. The Autotools backend runs configure,
Make, `make check`, and a staged install. It then verifies the declared commands
from a newly extracted artifact after removing the compilation directory.

```sh
rb package build xz --output /tmp/rootbeer-xz --jobs 2
rb apply --script /tmp/rootbeer-xz/install.lua
```

The output directory must not exist. A successful build produces `package.tar.gz`,
`package.json`, `install.lua`, and `receipt.json`. The receipt records the catalog
digest, recipe revision, source hash, platform, build dependencies, resolver inputs,
host toolchain, artifact hash, and output tree hash. It is written after command
checks pass. Logs remain in the output directory on failure; incomplete outputs
must not be published.

`build.dependencies` lists exact canonical requests such as `make@4.4.1` once that
recipe exists. Dependencies are validated for missing versions and cycles, then
built or downloaded in dependency order. Their exported commands are placed ahead
of system tools on the build PATH; command collisions fail. These are **build
dependencies**, not a runtime library linker or a general version solver.

Builds run with a cleared environment and the host `/usr/bin/cc` and system tools.
They execute trusted upstream code and are **not OS-sandboxed or hermetic**.
The recipe fixes source bytes, but recording compiler versions does not pin the
compiler or SDK. Archive metadata is normalized; byte-identical compilation across
hosts is not promised. Linux libc and macOS SDK baselines still need qualification.

`rb.package("xz")` fails until a binary is published; it never silently invokes a
compiler. Building is explicitly requested through `rb package build`, and the
generated installation script consumes the already-built artifact. This keeps
ordinary installation independent of the builder's compiler and Make.
