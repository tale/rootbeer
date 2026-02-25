# Development Setup

## Prerequisites

Rootbeer uses [mise](https://mise.jdx.dev/) to manage toolchain versions.
Install mise, then run:

```bash
mise install
```

This installs the pinned versions of:

- **Rust** (with rustfmt, clippy, and rust-analyzer)
- **Node + pnpm** (for the docs site)
- **lua-language-server** (for Lua type checking and doc generation)
- **lefthook** (git hooks)

Alternatively, if you have [Nix](https://nixos.org/) installed, you can use the
provided flake to get a development shell with all dependencies:

```bash
nix develop
```

Or with [direnv](https://direnv.net/), the `.envrc` will automatically load
the shell when you enter the project directory.

## Building

### With Cargo

```bash
# Debug build (binary lands in target/debug/rb)
cargo build

# Release build
cargo build --release
```

The debug binary is automatically on your `PATH` via the `mise.toml` env
config, so you can run `rb` directly after building.

### With Nix

```bash
# Build the package
nix build

# Run checks (clippy + fmt)
nix flake check
```

The binary is built to `./result/bin/rb`.

## Running Tests

```bash
cargo test --workspace
```

The test fixture lives in `test/` — `rootbeer.lua` is a sample manifest and
`dotfiles/` contains files used for symlink tests.

## Docs Site

The documentation site is built with [VitePress](https://vitepress.dev/).
API reference pages are auto-generated from Lua doc comments via
`scripts/lua2md.ts`.

```bash
# Install dependencies
pnpm install

# Start the dev server (auto-runs lua2md first)
pnpm dev

# Production build
pnpm build
```

## Git Hooks

Install the pre-commit and pre-push hooks with:

```bash
lefthook install
```

This runs automatically on commit and push:

| Hook | Checks |
|------|--------|
| `pre-commit` | `cargo fmt --check`, `cargo clippy` |
| `pre-push` | `cargo test --workspace` |

## Editor Setup

For the best experience working on Lua modules, configure your editor to use
**lua-language-server** with the `lua/` directory. The `@meta` file at
`lua/rootbeer/core.lua` provides type information for all native bindings,
giving you autocomplete and diagnostics in the high-level modules.
