local rb = require("rootbeer")
local M = {}

function M.create_config(config)
	local zsh_conf = rb.interpolate_table(config, function(cfg)
		local zshrc = ""

		-- Add environment variables
		if cfg.env then
			for key, value in pairs(cfg.env) do
				zshrc = zshrc .. "export " .. key .. "=\"" .. value .. "\"\n"
			end
		end

		-- Add aliases
		if cfg.aliases then
			for alias, command in pairs(cfg.aliases) do
				zshrc = zshrc .. "alias " .. alias .. "=\"" .. command .. "\"\n"
			end
		end

		return zshrc
	end)

	return rb.write_file("./test/.zshrc", zsh_conf)
end

return M
