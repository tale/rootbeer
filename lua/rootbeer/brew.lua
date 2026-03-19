--- @class brew
local M = {}

local rb = require("@rootbeer")

--- @class brew.Config
--- @field path? string Where to write the Brewfile. Defaults to `"~/.config/Brewfile"`.
--- @field formulae? string[] Homebrew formulae to install.
--- @field casks? string[] Homebrew casks to install.
--- @field mas? brew.MasApp[] Mac App Store apps to install.

--- @class brew.MasApp
--- @field name string Display name of the app.
--- @field id number Mac App Store ID.

--- Generates a Brewfile at `cfg.path` and runs `brew bundle` to apply it.
--- @param cfg brew.Config
function M.config(cfg)
	local path = cfg.path or "~/.config/Brewfile"
	local lines = {}

	if cfg.formulae then
		for _, formula in ipairs(cfg.formulae) do
			lines[#lines + 1] = 'brew "' .. formula .. '"'
		end
	end

	if cfg.casks then
		if #lines > 0 then
			lines[#lines + 1] = ""
		end
		for _, cask in ipairs(cfg.casks) do
			lines[#lines + 1] = 'cask "' .. cask .. '"'
		end
	end

	if cfg.mas then
		if #lines > 0 then
			lines[#lines + 1] = ""
		end
		for _, app in ipairs(cfg.mas) do
			lines[#lines + 1] = 'mas "' .. app.name .. '", id: ' .. app.id
		end
	end

	rb.file(path, table.concat(lines, "\n") .. "\n")
	rb.exec("brew", { "bundle", "--file=" .. path })
end

return M
