return {
	schema = 2,
	name = "ripgrep",
	aliases = { "rg" },
	description = "Search file contents with regular expressions",
	default_version = "15.2.0",
	homepage = "https://github.com/BurntSushi/ripgrep",
	systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
	inputs = {
		prebuilt = {
			github = "BurntSushi/ripgrep",
			tag = "15.2.0",
			assets = {
				["aarch64-linux"] = "ripgrep-15.2.0-aarch64-unknown-linux-musl.tar.gz",
				["aarch64-macos"] = "ripgrep-15.2.0-aarch64-apple-darwin.tar.gz",
				["x86_64-linux"] = "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz",
			},
		},
	},
	outputs = {
		bins = { "rg" },
		checks = { { "rg", "--version" } },
	},
	versions = {
		["15.2.0"] = {
			revision = 3,
			systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
			inputs = {
				prebuilt = {
					github = "BurntSushi/ripgrep",
					tag = "15.2.0",
					assets = {
						["x86_64-linux"] = "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz",
						["aarch64-macos"] = "ripgrep-15.2.0-aarch64-apple-darwin.tar.gz",
						["aarch64-linux"] = "ripgrep-15.2.0-aarch64-unknown-linux-musl.tar.gz",
					},
				},
			},
			outputs = {
				bins = { "rg" },
				checks = { { "rg", "--version" } },
			},
		},
	},
}
