---
outline: deep
---

# ssh

Declarative SSH configuration. Define your includes, host blocks, and global
options as a Lua table — rootbeer generates the `~/.ssh/config` file for you.
Booleans are automatically rendered as `yes`/`no`.

```lua
local ssh = require("@rootbeer/ssh")
```

## Examples

### Basic

```lua
ssh.config({
    hosts = {
        ["*"] = {
            ForwardAgent = true,
            Compression = true,
        },
    },
})
```

### Full featured

```lua
local rb = require("@rootbeer")

local includes = {}
if rb.path_exists("~/.orbstack/ssh/config") then
    includes[#includes + 1] = "~/.orbstack/ssh/config"
end
includes[#includes + 1] = "~/.ssh/private.config"

ssh.config({
    includes = includes,
    hosts = {
        ["*"] = {
            IdentityAgent = "SSH_AUTH_SOCK",
            ForwardAgent = true,
            Compression = true,
            ServerAliveInterval = 0,
            ServerAliveCountMax = 3,
            HashKnownHosts = false,
            UserKnownHostsFile = "~/.ssh/known_hosts",
            ControlMaster = "auto",
            ControlPath = "/tmp/ssh-%C",
            ControlPersist = true,
        },
    },
})
```

## API Reference

<!--@include: ./_generated/ssh.md-->
