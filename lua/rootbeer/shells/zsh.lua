local rb = require("rootbeer")
local M = {}

function M.env(key, value)
	rb.line("export " .. key .. '="' .. value .. '"')
end

function M.alias(name, command)
	rb.line("alias " .. name .. '="' .. command .. '"')
end

function M.source(path)
	rb.line("source " .. path)
end

function M.eval(command)
	rb.line('eval "$(' .. command .. ')"')
end

function M.raw(text)
	rb.emit(text)
end

function M.comment(text)
	rb.line("# " .. text)
end

function M.create_config(config)
	if config.env then
		for key, value in pairs(config.env) do
			M.env(key, value)
		end
	end

	if config.aliases then
		for name, command in pairs(config.aliases) do
			M.alias(name, command)
		end
	end

	if config.sources then
		for _, path in ipairs(config.sources) do
			M.source(path)
		end
	end

	if config.evals then
		for _, cmd in ipairs(config.evals) do
			M.eval(cmd)
		end
	end
end

return M
