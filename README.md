# Rootbeer

> Manage your dotfiles with Lua!

Rootbeer is a dotfile manager that lets you define your system configuration
in Lua scripts. Think [chezmoi](https://www.chezmoi.io/), but with the full
power of a real scripting language instead of Go templates.

**[Documentation →](https://rootbeer.tale.me)**

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
    evals = { "mise activate zsh" },
})

-- Conditionals — it's just Lua
if rb.host.os == "macos" then
    local brew = require("rootbeer.brew")
    brew.config({
        taps = { "homebrew/cask-fonts" },
        formulae = { "lsd", "delta", "mise" },
    })
end
```

## Key Ideas

- **Config is code** — Lua, not templates. Loops, conditionals, functions, and modules.
- **Plan & apply** — `rb.file()`, `rb.link_file()`, and module calls queue operations. Nothing touches the filesystem until `rb apply`.
- **Declarative modules** — zsh, git, SSH, Homebrew, macOS, and more. Describe the end state as a table, rootbeer generates the files.
- **First-class profiles** — Declare valid profiles, resolve them from simple string matchers, and branch with `rb.profile.select`, `rb.profile.when`, and `rb.profile.config`. CLI typos get suggestions.
- **Editor support** — `rb lsp` sets up lua-language-server for full autocomplete and type checking.

## Building

Requires Rust 1.79+.

```bash
cargo build           # → ./target/debug/rb
nix build             # → ./result/bin/rb
```

## License

MIT
