---
outline: deep
---

# Host

The `rb.host` table provides information about the current machine and user.
It is populated once at startup from POSIX syscalls — no environment variables
are used.

```lua
local rb = require("@rootbeer")

if rb.host.os == "macos" then
    -- macOS specific config
end

if rb.host.hostname == "work-macbook" then
    -- work machine overrides
end
```

## API Reference

<!--@include: ./_generated/host.md-->
