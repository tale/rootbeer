# claude_code

The Claude Code module manages `~/.claude/settings.json` and `CLAUDE.md` from
Lua. Use it for permissions, environment variables, hooks, model preferences,
and project instructions.

```lua
local claude_code = require("rootbeer.claude_code")
```

## Configure Claude Code

```lua
claude_code.config({
    model = "sonnet",
    lsp = { "typescript-lsp", "pyright-lsp" },
    permissions = {
        allow = {
            "Bash(npm run *)",
            "Bash(cargo *)",
            "Edit(*)",
        },
        deny = {
            "Read(.env)",
            "Read(.env.*)",
            "Bash(curl *)",
        },
    },
    env = {
        CLAUDE_CODE_ENABLE_TELEMETRY = "0",
    },
})
```

## Write Instructions

```lua
claude_code.prompt([[
## Conventions

- Use TypeScript with strict mode
- Prefer functional patterns over classes
- Always run tests before committing
- Use conventional commit messages
]], { lsp = true })
```

## LSP Support

Claude Code can use [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
for semantic code navigation — go-to-definition, find-references, hover types,
and real-time diagnostics.

The `lsp` field enables Claude Code's LSP tool and registers the plugins you
want to use:

```lua
claude_code.config({
    lsp = { "typescript-lsp", "pyright-lsp", "rust-analyzer-lsp" },
})
```

You still need to install both the language server binaries and the Claude Code
plugins manually. Rootbeer manages the settings, not the installation:

```bash
# 1. Install the language server binary
npm i -g pyright

# 2. Update the plugin catalog and install
claude plugin marketplace update claude-plugins-official
claude plugin install pyright-lsp
```

Pass `{ lsp = true }` to `prompt()` and Rootbeer will append a short Code
Intelligence section to your `CLAUDE.md` that tells Claude to prefer LSP over
grep for definitions, references, and type info.

## API Reference

<!--@include: ../api/_generated/claude_code.md-->
