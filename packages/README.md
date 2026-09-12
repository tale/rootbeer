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
target/release/rb package --catalog packages export --registry tale/rootbeer-index --output /tmp/rootbeer-export
```

The native exporter uses temporary stores and profiles, executes declared commands,
and recreates outputs offline from cached downloads and unchanged locked facts.
It requires network access for initial resolution and downloads. Exported receipts
and the platform index appear only after every applicable recipe passes. Catalog CI runs on relevant pushes to `main` and pull requests targeting
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
artifacts. Publication bundles provide the separate consumable artifact index. Keep this collection here until its
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

`rb.package("xz")` requires an index with an available binary; it never silently
invokes a compiler. Building is explicitly requested through `rb package build`, and the
generated installation script consumes the already-built artifact. This keeps
ordinary installation independent of the builder's compiler and Make.

## Preparing a publication bundle

Contributors can validate and build a recipe directory using an existing `rb`:

```sh
rb package --catalog ./packages check
rb package --catalog ./packages build xz --output /tmp/rootbeer-xz
rb package --catalog ./packages bundle \
  --receipt /tmp/rootbeer-xz/receipt.json \
  --base-url https://packages.example/rootbeer/snapshots/example \
  --output /tmp/rootbeer-bundle
```

`--catalog` replaces the embedded collection for package subcommands; it does not
change `rb apply` resolution. The directory uses the same Lua sandbox and schema
validation as the embedded collection. `rb package index` still exports recipes.

Repeat `--receipt` to combine successful builds from native platform runners.
Keep each receipt beside its `package.tar.gz`; recorded builder paths are ignored.
All receipts must match the selected catalog, recipe revision, build inputs, and
command contract. Duplicate package/version/platform entries fail. The bundler
verifies archive and installed-tree hashes without executing binaries, so artifacts
from different platforms can be assembled on one runner.

The new output directory contains `index.json`, `index.sha256`,
`artifacts/<sha256>.tar.gz`, and `receipts/<sha256>.json`. The index includes the
catalog snapshot and an `artifacts` map keyed by `name@version`, then system.
Only supplied builds appear in that map; a catalog platform declaration alone does
not advertise an available binary. Artifact URLs use `--base-url`. Receipts retain
original build provenance, including builder-local paths. Index bytes are stable
for identical inputs and base URL regardless of receipt argument order.

This command prepares files; it does not upload them. Select the resulting index
with `rb.package_index({ url = "https://.../index.json", sha256 = "<digest>" })`
before declaring packages. Local index files can use absolute `file:///` URLs;
artifact URLs still refer to the configured HTTPS host or GHCR repository. The printed SHA-256 covers the exact index bytes and is not a signature.
Receipts are build records, not authenticated attestations: publication must accept
outputs only from trusted, successful CI jobs. The client can verify an official signed manifest and cache snapshots automatically;
GitHub publication and the release endpoint/public key remain to be configured.

## GHCR Archives

Use a GHCR repository as the bundle destination:

```sh
rb package bundle --receipt /tmp/rootbeer-xz/receipt.json \
  --base-url ghcr://tale/rootbeer-packages/xz --output /tmp/rootbeer-bundle
```

The index records `ghcr://tale/rootbeer-packages/xz@sha256:<archive digest>`.
The digest identifies the actual archive blob, **not** its enclosing OCI manifest.
The publisher must upload the archive unchanged and retain an OCI manifest that
references the blob. Keep those manifests reachable when publishing newer versions;
older lockfiles must continue to work. Bundling still only prepares local files.

`rb` obtains an anonymous repository-scoped pull token directly from GHCR and
streams the blob through the normal hash-verified download cache. Public packages
need no Docker, ORAS, GitHub login, or user token. Private registry authentication
is not implemented. Offline replay uses cached bytes without contacting GHCR.

Rootbeer's binary releases remain separate from package distribution. The planned
package repository owns recipes, build/publish workflows, and signed snapshots
served by GitHub Pages; GHCR owns archive blobs. Publishing packages should never
create releases in `tale/rootbeer`. The existing documentation and nightly binary
site also stays separate from the package-index Pages deployment.
