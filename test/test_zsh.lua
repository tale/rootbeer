local zsh = require("rootbeer.shells.zsh")

local output = zsh.config({
    env = {
        EDITOR = "nvim",
        PATH = "/usr/local/bin:$PATH",
    },
    aliases = {
        ll = "ls -la",
        gs = "git status",
    },
    evals = {
        "mise activate zsh",
    },
    extra = "# End of rootbeer-generated config",
})

print(output)
