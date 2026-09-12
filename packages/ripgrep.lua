return {
	name = "ripgrep",
	aliases = { "rg" },
	description = "Search file contents with regular expressions",
	homepage = "https://github.com/BurntSushi/ripgrep",
	default_version = "15.2.0",
	versions = {
		["15.2.0"] = {
			revision = 1,
			source = "aqua:BurntSushi/ripgrep@15.2.0",
			systems = {
				"aarch64-macos",
				"x86_64-macos",
				"aarch64-linux",
				"x86_64-linux",
			},
			bins = { "rg" },
			checks = { { "rg", "--version" } },
		},
	},
}
