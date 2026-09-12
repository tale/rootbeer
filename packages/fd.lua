return {
	name = "fd",
	description = "Find files by name",
	homepage = "https://github.com/sharkdp/fd",
	default_version = "10.5.0",
	versions = {
		["10.5.0"] = {
			revision = 1,
			source = "github:sharkdp/fd@v10.5.0",
			assets = {
				["aarch64-macos"] = "fd-v10.5.0-aarch64-apple-darwin.tar.gz",
				["x86_64-macos"] = "fd-v10.5.0-x86_64-apple-darwin.tar.gz",
				["aarch64-linux"] = "fd-v10.5.0-aarch64-unknown-linux-musl.tar.gz",
				["x86_64-linux"] = "fd-v10.5.0-x86_64-unknown-linux-musl.tar.gz",
			},
			systems = {
				"aarch64-macos",
				"x86_64-macos",
				"aarch64-linux",
				"x86_64-linux",
			},
			bins = { "fd" },
			checks = { { "fd", "--version" } },
		},
		["10.4.2"] = {
			revision = 2,
			source = "github:sharkdp/fd@v10.4.2",
			assets = {
				["aarch64-macos"] = "fd-v10.4.2-aarch64-apple-darwin.tar.gz",
				["aarch64-linux"] = "fd-v10.4.2-aarch64-unknown-linux-musl.tar.gz",
				["x86_64-linux"] = "fd-v10.4.2-x86_64-unknown-linux-musl.tar.gz",
			},
			systems = {
				"aarch64-macos",
				"aarch64-linux",
				"x86_64-linux",
			},
			bins = { "fd" },
			checks = { { "fd", "--version" } },
		},
	},
}
