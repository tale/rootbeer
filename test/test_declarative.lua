local rb = require("rootbeer")
local zsh = require("rootbeer.shells.zsh")
local d = rb.data()

-- Declarative config
local env = {
	EDITOR = "nvim",
	LANG = "en_US.UTF-8",
}

if d.os == "Darwin" then
	env.HOMEBREW_PREFIX = "/opt/homebrew"
end

-- rb.file writes to disk, zsh.config renders a table to a string
rb.file("/tmp/rb-test-zshrc", zsh.config {
	env = env,
	aliases = {
		g = "git",
		v = "nvim",
		ls = "ls -la",
	},
	evals = { "/opt/homebrew/bin/brew shellenv" },
	sources = { "~/.config/zsh/local.zsh" },
	extra = {
		"autoload -Uz compinit && compinit",
		[[
# fzf integration
if [ -f ~/.fzf.zsh ]; then
    source ~/.fzf.zsh
fi]],
	},
})

-- Verify it was written
print("\n--- Written file contents ---")
local f = io.open("/tmp/rb-test-zshrc", "r")
if f then
	print(f:read("*a"))
	f:close()
end
