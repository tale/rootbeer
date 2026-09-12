return {
	name = "age",
	description = "File encryption with explicit recipients",
	homepage = "https://age-encryption.org/",
	default_version = "1.3.1",
	versions = {
		["1.3.1"] = {
			revision = 1,
			source = "aqua:FiloSottile/age@v1.3.1",
			systems = {
				"aarch64-macos",
				"x86_64-macos",
				"aarch64-linux",
				"x86_64-linux",
			},
			bins = { "age", "age-keygen" },
			checks = { { "age", "--version" }, { "age-keygen", "--version" } },
		},
	},
}
