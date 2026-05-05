--- @meta

--- @class profile.Spec
--- @field hosts? string[] Hostnames that resolve to this profile under the `"hostname"` strategy.
--- @field users? string[] Usernames that resolve to this profile under the `"user"` strategy.

--- @class profile.Ctx
--- @field match_hostname fun(): string?
--- @field match_user fun(): string?

--- @alias profile.Strategy "cli" | "hostname" | "user" | fun(ctx: profile.Ctx): string|nil

--- @class profile.Setup
--- @field strategy profile.Strategy How the active profile is chosen when `--profile` is not passed.
--- @field profiles table<string, profile.Spec> The set of valid profiles.

--- @class profile
local profile = {}

--- Declare the set of valid profiles and the resolution strategy. Call once
--- near the top of your config to enable the profile system.
---
--- ```lua
--- rb.profile.define({
---   strategy = "hostname",
---   profiles = {
---     personal = { hosts = { "Aarnavs-MBP" } },
---     work     = { hosts = { "atale-mbp" } },
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
