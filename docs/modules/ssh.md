# ssh

Declarative SSH configuration. Define your includes, host blocks, and global
options as a Lua table — rootbeer generates the `~/.ssh/config` file for you.
Booleans are automatically rendered as `yes`/`no`.

```lua
local ssh = require("rootbeer.ssh")
```

## Example

```lua
ssh.config({
    includes = { "~/.ssh/private.config" },
    hosts = {
        ["*"] = {
            ForwardAgent = true,
            Compression = true,
            ControlMaster = "auto",
            ControlPath = "/tmp/ssh-%C",
            ControlPersist = true,
        },
    },
})
```

## API Reference

<!--@include: ../api/_generated/ssh.md-->