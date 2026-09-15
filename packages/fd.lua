return {
	schema = 2,
	name = "fd",
	description = "Find files by name",
	default_version = "10.5.0",
	homepage = "https://github.com/sharkdp/fd",
	systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
	inputs = {
		prebuilt = {
			github = "sharkdp/fd",
			tag = "v10.5.0",
			assets = {
				["aarch64-linux"] = "fd-v10.5.0-aarch64-unknown-linux-musl.tar.gz",
				["aarch64-macos"] = "fd-v10.5.0-aarch64-apple-darwin.tar.gz",
				["x86_64-linux"] = "fd-v10.5.0-x86_64-unknown-linux-musl.tar.gz",
			},
		},
	},
	outputs = {
		bins = { "fd" },
		checks = { { "fd", "--version" } },
	},
	versions = {
		["10.4.2"] = {
			revision = 2,
			systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
			inputs = {
				prebuilt = {
					github = "sharkdp/fd",
					tag = "v10.4.2",
					assets = {
						["x86_64-linux"] = "fd-v10.4.2-x86_64-unknown-linux-musl.tar.gz",
						["aarch64-macos"] = "fd-v10.4.2-aarch64-apple-darwin.tar.gz",
						["aarch64-linux"] = "fd-v10.4.2-aarch64-unknown-linux-musl.tar.gz",
					},
				},
			},
			outputs = {
				bins = { "fd" },
				checks = { { "fd", "--version" } },
			},
		},
		["10.5.0"] = {
			revision = 2,
			systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
			inputs = {
				prebuilt = {
					github = "sharkdp/fd",
					tag = "v10.5.0",
					assets = {
						["x86_64-linux"] = "fd-v10.5.0-x86_64-unknown-linux-musl.tar.gz",
						["aarch64-macos"] = "fd-v10.5.0-aarch64-apple-darwin.tar.gz",
						["aarch64-linux"] = "fd-v10.5.0-aarch64-unknown-linux-musl.tar.gz",
					},
				},
			},
			outputs = {
				bins = { "fd" },
				checks = { { "fd", "--version" } },
			},
		},
	},
}
