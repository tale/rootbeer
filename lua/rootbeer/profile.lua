--- @class profile
local M = {}

local rb = require("rootbeer")

--- Collects and sorts the keys of the given map for error messages.
--- @param map table<string, any>
--- @return string[]
local function sorted_keys(map)
	local keys = {}
	for k in pairs(map) do
		keys[#keys + 1] = k
	end
	table.sort(keys)
	return keys
end

--- Selects and requires a module based on the active profile.
--- Each key in the map is a profile name and its value is a path to a
--- `.lua` file relative to the script directory. All paths are validated
--- upfront before the selected profile is executed.
--- When `rb.profile` is `nil` the call is a no-op.
---
--- ```lua
--- local profile = require("rootbeer.profile")
---
--- profile.config({
---   work = "hosts/work.lua",
---   personal = "hosts/personal.lua",
--- })
--- ```
--- @param map table<string, string> Profile name → `.lua` file path.
function M.config(map)
	for name, path in pairs(map) do
		if not rb.is_file(path) then
			error("profile '" .. name .. "': file not found: " .. path)
		end
	end

	local name = rb.profile
	if name == nil then
		return
	end

	local path = map[name]
	if path == nil then
		error("unknown profile '" .. name .. "', expected one of: " .. table.concat(sorted_keys(map), ", "))
	end

	require(path:gsub("%.lua$", ""))
end

--- Returns the value from `map` that matches the active profile.
--- If the active profile is not in `map`, falls back to `map.default`.
--- Errors when the profile has no matching entry and no default is set.
--- When `rb.profile` is `nil`, returns the `default` value or errors.
---
--- ```lua
--- local profile = require("rootbeer.profile")
---
--- local email = profile.select({
---   default = "aarnav@personal.me",
---   work = "aarnav@company.com",
--- })
--- ```
--- @param map table<string, any> Profile name → value. Use `"default"` as the fallback key.
--- @return any
function M.select(map)
	local name = rb.profile

	if name ~= nil and map[name] ~= nil then
		return map[name]
	end

	if map.default ~= nil then
		return map.default
	end

	if name == nil then
		error("profile.select: no profile is active and no default was provided")
	end

	error("profile.select: no match for profile '" .. name .. "' and no default (known: " .. table.concat(sorted_keys(map), ", ") .. ")")
end

--- Runs `fn` only when the active profile matches.
--- Accepts a single profile name or a list of names.
--- When `rb.profile` is `nil` the call is a no-op.
---
--- ```lua
--- local profile = require("rootbeer.profile")
---
--- profile.when("personal", function()
---   mac.touch_id_sudo()
---   mac.hostname({ name = "Aarnavs-MBP" })
--- end)
---
--- profile.when({"work", "personal"}, function()
---   -- runs for either profile
--- end)
--- ```
--- @param names string|string[] One or more profile names.
--- @param fn fun() The function to execute if the profile matches.
function M.when(names, fn)
	local name = rb.profile
	if name == nil then
		return
	end

	if type(names) == "string" then
		if name == names then
			fn()
		end
		return
	end

	for _, n in ipairs(names) do
		if name == n then
			fn()
			return
		end
	end
end

return M
