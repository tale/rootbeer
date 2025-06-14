local M = {}
local zsh = require("rootbeer.shells.zsh")

function M.create_zsh_config(config)
	return zsh.create_config(config)
end

return M
