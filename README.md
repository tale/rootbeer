# Rootbeer

> Declare your packages and system configuration in Lua.

Rootbeer manages your tools, dotfiles, and shell settings from a Lua configuration.
Keep it in Git and use it to set up your macOS and Linux machines.

**[Documentation](https://rbpkg.com) · [Package catalog](https://rbpkg.com/packages/)**

## Quick Start

Install and bootstrap in one command:

```bash
# Clone an existing dotfiles repo
sh -c "$(curl -fsSL rbpkg.com/rb.sh)" -- init tale/dotfiles

# Clone via SSH (if keys are already set up)
sh -c "$(curl -fsSL rbpkg.com/rb.sh)" -- init --ssh tale/dotfiles

# Or start fresh
sh -c "$(curl -fsSL rbpkg.com/rb.sh)" -- init
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

rb.package("ripgrep")

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
})

```

## Packages

[Find packages](https://rbpkg.com/packages/) to add to `init.lua`, then run
`rb apply`, then run `eval "$(rb env)"` to use the installed commands.
The Zsh example above also loads them automatically in Zsh login shells.
Commit `rootbeer.lock` to save your package versions, and run `rb apply --update`
when you want to update them. See the
[package guide](https://rbpkg.com/guide/packages) for other shells and version selection.

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
