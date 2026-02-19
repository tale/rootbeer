local rb = require("rootbeer")
local zsh = require("rootbeer.shells.zsh")
local d = rb.data()

-- Write a generated config
rb.file("~/.config/rootbeer-test/zshrc", zsh.config {
	env = { EDITOR = "nvim" },
	aliases = { g = "git" },
	extra = "autoload -Uz compinit && compinit",
})

-- Symlink a source file to a target path
rb.link_file("test.bash", "~/.config/rootbeer-test/test.bash")
