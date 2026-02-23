local rb = require("@rootbeer")
local zsh = require("@rootbeer/zsh")

print("rootbeer test run")
for k, v in pairs(rb.host) do
	print("rb.host." .. k .. " = " .. tostring(v))
end

-- Write a generated zshrc using the shell module
zsh.config({
	path = "/tmp/rb-test/zshrc",
	keybind_mode = "emacs",
	options = { "CORRECT", "EXTENDED_GLOB" },
	env = {
		EDITOR = "nvim",
		LANG = "en_US.UTF-8",
	},
	aliases = {
		g = "git",
		ls = "ls -la",
	},
	history = {},
	evals = {
		"mise activate zsh",
	},
	sources = {
		"~/.config/zsh/local.zsh",
	},
})

-- Conditional based on OS
if rb.host.os == "macos" then
	rb.file("/tmp/rb-test/darwin.txt", "this is a mac\n")
elseif rb.host.os == "linux" then
	rb.file("/tmp/rb-test/linux.txt", "this is linux\n")
end

-- Symlink a static file
rb.link_file("dotfiles/gitconfig", "/tmp/rb-test/gitconfig")

print("done")
