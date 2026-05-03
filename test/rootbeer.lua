local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

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

-- Writer smoke test: round-trip JSON / TOML, exercise INI encode + write
local sample = { name = "rootbeer", tags = { "dotfiles", "lua" }, count = 3 }

local json_str = rb.json.encode(sample)
local json_back = rb.json.decode(json_str)
assert(json_back.name == "rootbeer", "json round-trip failed")
assert(#json_back.tags == 2, "json array round-trip failed")

local toml_str = rb.toml.encode(sample)
local toml_back = rb.toml.decode(toml_str)
assert(toml_back.count == 3, "toml round-trip failed")

rb.json.write("/tmp/rb-test/sample.json", sample)
rb.toml.write("/tmp/rb-test/sample.toml", sample)
rb.ini.write("/tmp/rb-test/sample.ini", { section = { key = "value" } })

print("done")
