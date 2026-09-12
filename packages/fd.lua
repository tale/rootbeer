return {
	name = "fd",
	description = "Find files by name",
	homepage = "https://github.com/sharkdp/fd",
	default_version = "10.4.2",
	versions = {
		["10.4.2"] = {
			revision = 1,
			source = "aqua:sharkdp/fd@v10.4.2",
			systems = {
				"aarch64-macos",
				"x86_64-macos",
				"aarch64-linux",
				"x86_64-linux",
			},
			bins = { "fd" },
			checks = { { "fd", "--version" } },
		},
	},
}
