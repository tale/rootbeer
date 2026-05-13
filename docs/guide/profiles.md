# Profiles

::: info
Profiles are an optional but powerful way to manage different machines with a
single config repo. If you don't have a need for multiple profiles, you can
skip this page and keep things simple.
:::

A **profile** is a named role for the current machine: `personal`, `work`,
`server`, `ci`, or whatever makes sense for your setup. Profiles model what a
machine is *for*. Host fields like OS, arch, hostname, and user model what the
machine *is*.

## The Problem Profiles Solve

Imagine you have a Git config that sets your name and email. If you want to use
different emails on your work and personal machine you might do this:

```lua
local rb = require("rootbeer")
local git = require("rootbeer.git")

local user_config = {
    name = "Aarnav Tale",
    email = "git@tale.me",
}

if rb.host.hostname == "tale-work" then
    user_config.email = "atale@work.com"
end

git.config({
    user = user_config,
    editor = "nvim",
    -- Rest of your config...
})
```

This works, and Rootbeer is happy to let you use plain Lua when that's the best
fit. The downside is that the branches tend to spread out as your config grows:

- Every config that differs by machine needs its own `if` statement.
- Hostnames end up repeated across unrelated files.
- It's hard to see the full list of supported machines and roles in one place.

Profiles make that branching explicit. You declare the roles your config knows
about once, choose how Rootbeer should detect the active role, and then branch
on the active profile wherever values differ.

## Define Your Profiles

To start, you'll need to define the profiles that you want to cover in your
config. It's generally a good idea to do this at the top of your `init.lua`
file, since the active profile is used by the rest of your config.

```lua
local rb = require("rootbeer")

rb.profile.define({
    strategy = "hostname",
    profiles = {
        personal = { "Aarnavs-MBP" },
        work     = { "tale-work" },
        server   = { "server-1", "server-2" },
    },
})
```

Each key under `profiles` is a valid profile name. Each string in the list is a
value that can resolve to that profile. With `strategy = "hostname"`, those
strings are hostnames.

## Use the Active Profile

For small differences, use `rb.profile.select()` to choose a value inline:

```lua
local git = require("rootbeer.git")

git.config({
    user = {
        name = "Aarnav Tale",
        email = rb.profile.select({
            default = "aarnav@personal.me",
            work    = "aarnav@company.com",
        }),
    },
})
```

`default` is the fallback when the active profile doesn't have its own value.
For larger differences, use `rb.profile.when()` to run a whole block for one or
more profiles:

```lua
local mac = require("rootbeer.mac")

rb.profile.when("personal", function()
    mac.hostname({ name = "Aarnavs-MBP" })
    mac.touch_id_sudo()
end)

rb.profile.when({ "work", "personal" }, function()
    mac.dock({ autohide = true })
end)
```

## Split Larger Configs

Once a profile-specific section gets large enough, move it into its own file:

```lua
rb.profile.config({
    work     = "hosts/work.lua",
    personal = "hosts/personal.lua",
})
```

Paths are validated upfront, so a typo fails before Rootbeer applies any
changes.

## Choose a Strategy

The strategy decides how Rootbeer chooses the active profile. Most configs
should start with `strategy = "hostname"`: it lets each machine select its
profile automatically.

| Strategy     | Matches against                                   |
| ------------ | ------------------------------------------------- |
| `"hostname"` | `rb.host.hostname`                                |
| `"user"`     | `rb.host.user`                                    |
| `"command"`  | The names of executables on `PATH` (or absolute paths) |
| `"cli"`      | The `--profile` flag                              |
| `function`   | Whatever you return from your own logic           |

Use `strategy = "cli"` when you want to choose the profile manually every time:

```lua
rb.profile.define({
    strategy = "cli",
    profiles = {
        personal = {},
        work     = {},
    },
})
```

```bash
rb apply -p work
```

Use `strategy = "user"` for shared machines where the current username is a
better signal than the hostname.

Use `strategy = "command"` when work-issued machines ship corp tooling that a
personal machine would never have. Each matcher is a command name looked up on
`PATH`, or an absolute path. The first profile with a hit wins:

```lua
rb.profile.define({
    strategy = "command",
    profiles = {
        work     = { "iru", "spear-cli" },
        personal = {}, -- fallback when no work tools are present
    },
})
```

### Fallback Profiles

A profile declared with an empty matcher list is the **fallback**: when no
other profile matches, Rootbeer picks it. Only one profile may be the fallback
— if two profiles have empty lists, the choice is ambiguous and no automatic
fallback applies. (This is what makes `strategy = "cli"` with all-empty
matchers behave as expected: omitting `--profile` errors out instead of
silently picking one.)

### Custom Strategies

If you want full control, use a function. It can return any valid profile name
from regular Lua logic. Rootbeer passes in `ctx` for the common helpers, but
you don't have to use it for everything:

```lua
rb.profile.define({
    strategy = function(ctx)
        if rb.host.os == "linux" then
            return "server"
        end

        return ctx.command()
            or ctx.cli()
            or ctx.hostname()
            or "personal"
    end,
    profiles = {
        personal = { "Aarnavs-MBP" },
        work     = { "iru" },
        server   = {},
    },
})
```

The `--profile` flag is not a global override. It is used only by
`strategy = "cli"` or by a custom strategy that calls `ctx.cli()`.

## A Complete Example

```lua
local rb  = require("rootbeer")
local git = require("rootbeer.git")
local mac = require("rootbeer.mac")

rb.profile.define({
    strategy = "hostname",
    profiles = {
        personal = { "Aarnavs-MBP" },
        work     = { "atale-mbp" },
    },
})

git.config({
    user = {
        name  = "Aarnav Tale",
        email = rb.profile.select({
            default = "aarnav@personal.me",
            work    = "aarnav@company.com",
        }),
    },
})

rb.profile.when("personal", function()
    mac.touch_id_sudo()
    mac.hostname({ name = "Aarnavs-MBP" })
end)

rb.profile.config({
    work     = "hosts/work.lua",
    personal = "hosts/personal.lua",
})
```

## Host Detection vs. Profiles

Use profiles for everything that depends on what the machine is *for*. Use
`rb.host` for facts about the running system:

```lua
if rb.host.os == "macos" then
    require("modules.brew")
end
```

See the [Host reference](/reference/host) for all available fields.

## API Reference

<!--@include: ../api/_generated/profile.md-->
