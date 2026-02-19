local M = {}

--- Renders a declarative zsh config table into a string.
--- Supports: env, aliases, sources, evals, plugins, extra
--- @param cfg table The configuration table.
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
