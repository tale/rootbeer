# amp

Declarative Amp configuration. Manage your `~/.config/amp/settings.json`
from a Lua table — MCP servers, permissions, thinking mode, and more.

```lua
local amp = require("rootbeer.amp")
```

## Example

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
