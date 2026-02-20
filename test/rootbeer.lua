local rb = require("@rootbeer")
local zsh = require("@rootbeer.shells.zsh")
local d = rb.data()

print("rootbeer test run")
print("  os:       " .. d.os)
print("  arch:     " .. d.arch)
print("  hostname: " .. d.hostname)
print("  user:     " .. d.username)
print("")

-- Write a generated zshrc using the shell module
rb.file("/tmp/rb-test/zshrc", zsh.config({
	env = {
		EDITOR = "nvim",
		LANG = "en_US.UTF-8",
	},
	aliases = {
		g = "git",
		ls = "ls -la",
	},
	evals = {
		"mise activate zsh",
	},
	sources = {
		"~/.config/zsh/local.zsh",
	},
	extra = "autoload -Uz compinit && compinit",
}))

-- Conditional based on OS
if d.os == "macos" then
	rb.file("/tmp/rb-test/darwin.txt", "this is a mac\n")
elseif d.os == "linux" then
	rb.file("/tmp/rb-test/linux.txt", "this is linux\n")
end

-- Symlink a static file
rb.link_file("dotfiles/gitconfig", "/tmp/rb-test/gitconfig")

print("done")
