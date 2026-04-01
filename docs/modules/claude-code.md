# claude_code

Declarative Claude Code configuration. Manage your `~/.claude/settings.json`
from a Lua table — permissions, environment variables, hooks, and model
preferences.

```lua
local claude_code = require("rootbeer.claude_code")
```

## Example

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
and real-time diagnostics. This is **not enabled by default**; it requires an
env flag (`ENABLE_LSP_TOOL`) discovered via
[anthropics/claude-code#15619](https://github.com/anthropics/claude-code/issues/15619).

The `lsp` field handles all of this for you. Pass a list of plugin names and
rootbeer will:

1. Set `ENABLE_LSP_TOOL=1` in your settings env
2. Register each plugin as `name@claude-plugins-official` in `enabledPlugins`

You still need to install both the language server binaries and the Claude Code
plugins manually — rootbeer manages the settings, not the installation:

```bash
# 1. Install the language server binary
npm i -g pyright

# 2. Update the plugin catalog and install
claude plugin marketplace update claude-plugins-official
claude plugin install pyright-lsp
```

Available plugins: `typescript-lsp`, `pyright-lsp`, `gopls-lsp`,
`rust-analyzer-lsp`, `jdtls-lsp`, `clangd-lsp`, `csharp-lsp`, `php-lsp`,
`kotlin-lsp`, `swift-lsp`, `lua-lsp`.

```lua
claude_code.config({
    lsp = { "typescript-lsp", "pyright-lsp", "rust-analyzer-lsp" },
})
```

Pass `{ lsp = true }` to `prompt()` and rootbeer will automatically append
a "Code Intelligence" section to your `CLAUDE.md` that tells Claude to prefer
LSP over grep for definitions, references, and type info:

```lua
claude_code.prompt("Your instructions here", { lsp = true })
```

## API Reference

<!--@include: ../api/_generated/claude_code.md-->
