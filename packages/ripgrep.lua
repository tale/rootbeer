return {
	name = "ripgrep",
	aliases = { "rg" },
	description = "Search file contents with regular expressions",
	homepage = "https://github.com/BurntSushi/ripgrep",
	default_version = "15.2.0",
	versions = {
		["15.2.0"] = {
			revision = 2,
			source = "github:BurntSushi/ripgrep@15.2.0",
			assets = {
				["aarch64-macos"] = "ripgrep-15.2.0-aarch64-apple-darwin.tar.gz",
				["x86_64-macos"] = "ripgrep-15.2.0-x86_64-apple-darwin.tar.gz",
				["aarch64-linux"] = "ripgrep-15.2.0-aarch64-unknown-linux-musl.tar.gz",
				["x86_64-linux"] = "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz",
			},
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
