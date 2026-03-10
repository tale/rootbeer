--- @class profile
local M = {}

local rb = require("@rootbeer")

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
		local known = {}
		for k in pairs(map) do
			known[#known + 1] = k
		end
		table.sort(known)
		error("unknown profile '" .. name .. "', expected one of: " .. table.concat(known, ", "))
	end

	require(path:gsub("%.lua$", ""))
end

return M
