# Packaging

## Package Collection

Canonical CLI package definitions live in `packages/*.lua`. Adding a definition
requires no Rust changes. `rb package check` validates the embedded collection;
`rb package index` exports its deterministic JSON snapshot. Use
`rb package --catalog ./packages check` or `build` to work from recipe files with
an existing binary.

`rb package bundle --receipt <build>/receipt.json --base-url <https-url>
--output <new-directory>` assembles source artifacts, receipts, and an index for
hosting. Repeat `--receipt` for additional versions or platforms. It checks catalog
inputs and archive/output hashes without executing binaries. Only supplied builds
appear as available artifacts. Bundling does not upload files or enable client
index consumption.

See the [collection authoring guide](https://github.com/tale/rootbeer/tree/main/packages)
for the recipe format and local smoke checks. The package workflow tests native
Linux and macOS installs on Intel and ARM, followed by offline profile recreation.
It runs only for public repositories and does not publish packages.

The catalog imports upstream release binaries and supports explicit Autotools
source builds with ordered build dependencies. The first source recipe is XZ.
Pinned toolchains, runtime dependencies, signed remote indexes, and a public binary
cache remain separate work.

## Rootbeer Distribution

Rootbeer ships as a single binary (`rb`) with an optional set of Lua standard
library files. How those files are delivered depends on the distribution
channel.

## Embedded Stdlib (Default)

By default, the `embedded-stdlib` Cargo feature is enabled. This bakes every
`lua/rootbeer/*.lua` module into the binary via `include_str!` at compile
time. The resulting binary is fully self-contained — no extra files to install,
no paths to configure.

```bash
cargo build --release
# target/release/rb is all you need
```

This is the recommended mode for Homebrew, Nix, and direct downloads.

## Separate Stdlib

Some Linux distributions (Debian, Fedora, Arch, etc.) have packaging policies
that require source files to remain on disk rather than embedded in binaries.
Disable the default feature to get this behavior:

```bash
cargo build --release --no-default-features
```

In this mode, the binary reads `rootbeer.*` modules from disk at runtime.
The path is set at compile time via the `ROOTBEER_LUA_DIR` environment
variable (defaults to the repo's `lua/` directory):

```bash
ROOTBEER_LUA_DIR=/usr/share/rootbeer/lua \
  cargo build --release --no-default-features
```

The expected directory layout at that path:

```
/usr/share/rootbeer/lua/
└── rootbeer/
    ├── brew.lua
    ├── core.lua
    ├── git.lua
    ├── host.lua
    ├── profile.lua
    ├── ssh.lua
    └── zsh.lua
```

Users can also override the path at runtime with `--lua-dir`:

```bash
rb --lua-dir /opt/rootbeer/lua apply
```

## Quick Reference

| Channel | Build command | Ships |
|---------|--------------|-------|
| Homebrew / Nix / direct | `cargo build --release` | Single binary |
| Distro packages | `cargo build --release --no-default-features` | Binary + `lua/` tree |
| Development | `cargo build` | Binary reads `lua/` from repo |
