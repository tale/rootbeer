--- @class ssh
local M = {}

local rb = require("@rootbeer")

--- @class ssh.Config
--- @field path? string Where to write the SSH config. Defaults to `"~/.ssh/config"`.
--- @field includes? string[] Paths added as `Include` directives at the top.
--- @field hosts? table<string, table<string, string|number|boolean>> Host blocks. Keys are match patterns (e.g. `"*"`, `"github.com"`), values are option tables.

--- Formats a scalar SSH config value.
--- Booleans become `yes`/`no`, everything else is stringified as-is.
--- @param value string|number|boolean
--- @return string
local function fmt(value)
	if type(value) == "boolean" then
		return value and "yes" or "no"
	end
	return tostring(value)
end

--- Generates an SSH config file from structured data.
--- @param cfg ssh.Config
function M.config(cfg)
	local path = cfg.path or "~/.ssh/config"
	local lines = {}

	if cfg.includes then
		for _, inc in ipairs(cfg.includes) do
			lines[#lines + 1] = "Include " .. inc
		end
	end

	if cfg.hosts then
		for pattern, opts in pairs(cfg.hosts) do
			if #lines > 0 then
				lines[#lines + 1] = ""
			end
			lines[#lines + 1] = "Host " .. pattern
			for key, value in pairs(opts) do
				lines[#lines + 1] = "    " .. key .. " " .. fmt(value)
			end
		end
	end

	rb.file(path, table.concat(lines, "\n") .. "\n")
end

return M
