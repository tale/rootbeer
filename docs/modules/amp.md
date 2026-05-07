# amp

The amp module manages Amp settings and project instructions from Lua. Use it
for `~/.config/amp/settings.json`, MCP servers, permissions, update behavior,
and `AGENTS.md` content.

```lua
local amp = require("rootbeer.amp")
```

## Configure Amp

```lua
amp.config({
    thinking = true,
    updates_mode = "warn",
    mcp_servers = {
        playwright = {
            command = "npx",
            args = { "-y", "@playwright/mcp@latest", "--headless" },
        },
    },
    permissions = {
        { tool = "Bash(npm run *)", mode = "allow" },
        { tool = "Bash(rm *)", mode = "ask" },
    },
})
```

## Write Instructions

Use `amp.prompt()` for the instructions you want Amp to follow in this config:

```lua
amp.prompt([[
## Conventions

- Use TypeScript with strict mode
- Prefer functional patterns over classes
- Always run tests before committing
- Use conventional commit messages
]])
```

## API Reference

<!--@include: ../api/_generated/amp.md-->
