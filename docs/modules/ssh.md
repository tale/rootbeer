# ssh

The ssh module manages `~/.ssh/config` from Lua. Define includes, global
options, and host blocks in one place; Rootbeer renders the OpenSSH config for
you.

```lua
local ssh = require("rootbeer.ssh")
```

## Configure SSH

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

Boolean values are rendered as the `yes`/`no` strings OpenSSH expects.

## API Reference

<!--@include: ../api/_generated/ssh.md-->
