# Rootbeer

> Declare your packages and system configuration in Lua.

Rootbeer is a standalone tool for managing command-line packages, files, shell
settings, and machine-specific configuration together. Its signed package catalog
supplies verified platform binaries, while `rootbeer.lock` keeps installations
stable until you choose to update.

**[Documentation](https://rootbeer.tale.me) · [Package catalog](https://rootbeer.tale.me/packages/)**

## Quick Start

Install and bootstrap in one command:

```bash
# Clone an existing dotfiles repo
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init tale/dotfiles

# Clone via SSH (if keys are already set up)
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init --ssh tale/dotfiles

# Or start fresh
sh -c "$(curl -fsSL rootbeer.tale.me/rb.sh)" -- init
```

Then apply your configuration:

```bash
rb apply              # apply configuration
rb apply -n              # dry run (preview without writing)
rb apply -p personal     # provide a CLI profile input
```

## What It Looks Like

```lua
local rb = require("rootbeer")
local git = require("rootbeer.git")
local zsh = require("rootbeer.zsh")

rb.profile.define({
    strategy = "hostname",
    profiles = {
        personal = { "Aarnavs-MBP" },
        work     = { "atale-mbp" },
    },
})

git.config({
    user = {
        name = "Aarnav Tale",
        email = rb.profile.select({
            default = "aarnav@personal.me",
            work    = "aarnav@company.com",
        }),
    },
    editor = "nvim",
    signing = {
        key = "~/.ssh/id_ed25519.pub",
    },
})

zsh.config({
    env = { EDITOR = "nvim" },
    aliases = { g = "git", vim = "nvim" },
    prompt = '%F{cyan}%~%f %F{white}>%f ',
    history = { size = 10000 },
    sources = { rb.env_export("sh") },
})

```

## Packages

Find a canonical name in the package catalog, copy its Lua declaration, and run
`rb apply`. Source the generated package environment from your shell, and commit
`rootbeer.lock` with your config. Matching locks stay stable; `rb apply --update`
refreshes selections, and `rb apply --offline` replays cached contents.

The index publishes independently of the CLI. Available versions can differ by
platform, and exact version requests never silently fall back. See the
[package guide](https://rootbeer.tale.me/guide/packages) for the complete workflow.

## Key Ideas

- **Config is code** — Lua, not templates. Loops, conditionals, functions, and modules.
- **Plan & apply** — `rb.file()`, `rb.link_file()`, and module calls queue operations. Nothing touches the filesystem until `rb apply`.
- **Declarative modules** — zsh, git, SSH, Homebrew, macOS, and more. Describe the end state as a table, rootbeer generates the files.
- **First-class profiles** — Declare valid profiles, resolve them from simple string matchers, and branch with `rb.profile.select`, `rb.profile.when`, and `rb.profile.config`. CLI typos get suggestions.
- **Editor support** — `rb init` configures LuaLS autocomplete and type definitions.

## Building

Requires Rust 1.93+.

```bash
cargo build           # → ./target/debug/rb
nix build             # → ./result/bin/rb
```

## License

MIT
