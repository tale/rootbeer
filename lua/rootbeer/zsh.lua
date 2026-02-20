local M = {}

--- @class zsh.Config
--- @field env? table<string, string> Environment variables exported via `export KEY="value"`.
--- @field aliases? table<string, string> Shell aliases defined via `alias name="command"`.
--- @field evals? string[] Commands wrapped in `eval "$(cmd)"`.
--- @field sources? string[] File paths to source via `source path`.
--- @field extra? string|string[] Raw lines appended as-is to the output.

--- Renders a declarative zsh config table into a string.
--- @param cfg zsh.Config The configuration table.
--- @return string The rendered zshrc content.
function M.config(cfg)
	local lines = {}

	local function add(line)
		lines[#lines + 1] = line
	end

	if cfg.env then
		for key, value in pairs(cfg.env) do
			add('export ' .. key .. '="' .. value .. '"')
		end
	end

	if cfg.aliases then
		if #lines > 0 then add("") end
		for name, command in pairs(cfg.aliases) do
			add('alias ' .. name .. '="' .. command .. '"')
		end
	end

	if cfg.evals then
		if #lines > 0 then add("") end
		for _, cmd in ipairs(cfg.evals) do
			add('eval "$(' .. cmd .. ')"')
		end
	end

	if cfg.sources then
		if #lines > 0 then add("") end
		for _, path in ipairs(cfg.sources) do
			add("source " .. path)
		end
	end

	-- extra: string or table of strings, appended as-is
	if cfg.extra then
		if #lines > 0 then add("") end
		if type(cfg.extra) == "string" then
			add(cfg.extra)
		elseif type(cfg.extra) == "table" then
			for _, block in ipairs(cfg.extra) do
				add(block)
			end
		end
	end

	return table.concat(lines, "\n") .. "\n"
end

return M
