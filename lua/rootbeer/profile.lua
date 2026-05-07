--- @meta

--- Strings that resolve to a profile when matched by the active strategy.
--- `"hostname"` matches against `rb.host.hostname`; `"user"` matches
--- against `rb.host.user`; custom strategies can call `ctx.match(value)` or
--- compose built-in strategy helpers such as `ctx.cli()` and `ctx.hostname()`.
--- @alias profile.Matchers string[]

--- @class profile.Ctx
--- @field match fun(value: string?): string? Match an arbitrary string against the profile matcher table.
--- @field cli fun(): string? Return and validate the `--profile` value, or `nil` when omitted.
--- @field hostname fun(): string? Match `rb.host.hostname` against the profile matcher table.
--- @field user fun(): string? Match `rb.host.user` against the profile matcher table.

--- @alias profile.Strategy "cli" | "hostname" | "user" | fun(ctx: profile.Ctx): string?

--- @class profile.Setup
--- @field strategy profile.Strategy How the active profile is chosen.
--- @field profiles table<string, profile.Matchers> Profile name → exact strings that resolve to that profile.

--- @class profile
local profile = {}

--- Declare the set of valid profiles and the resolution strategy. Call once
--- near the top of your config to enable the profile system.
---
--- ```lua
--- rb.profile.define({
---   strategy = "hostname",
---   profiles = {
---     personal = { "Aarnavs-MBP" },
---     work     = { "atale-mbp" },
---   },
--- })
--- ```
--- @param spec profile.Setup
function profile.define(spec) end

--- Returns the active profile name, or `nil` when none is set.
--- @return string?
function profile.current() end

--- Returns the value from `map` keyed by the active profile, falling back
--- to `map.default`.
---
--- ```lua
--- local email = rb.profile.select({
---   default = "aarnav@personal.me",
---   work    = "aarnav@company.com",
--- })
--- ```
--- @param map table<string, any> Profile name → value. Use `"default"` as the fallback key.
--- @return any
function profile.select(map) end

--- Runs `fn` only when the active profile matches.
---
--- ```lua
--- rb.profile.when("personal", function() ... end)
--- rb.profile.when({ "work", "personal" }, function() ... end)
--- ```
--- @param names string|string[] One or more profile names.
--- @param fn fun() The function to execute if the active profile matches.
function profile.when(names, fn) end

--- Requires a per-profile `.lua` file. Paths are validated upfront.
---
--- ```lua
--- rb.profile.config({
---   work     = "hosts/work.lua",
---   personal = "hosts/personal.lua",
--- })
--- ```
--- @param map table<string, string> Profile name → `.lua` file path.
function profile.config(map) end

return profile
