return {
	name = "age",
	description = "File encryption with explicit recipients",
	homepage = "https://age-encryption.org/",
	default_version = "1.3.1",
	versions = {
		["1.3.1"] = {
			revision = 2,
			source = "github:FiloSottile/age@v1.3.1",
			assets = {
				["aarch64-macos"] = "age-v1.3.1-darwin-arm64.tar.gz",
				["x86_64-macos"] = "age-v1.3.1-darwin-amd64.tar.gz",
				["aarch64-linux"] = "age-v1.3.1-linux-arm64.tar.gz",
				["x86_64-linux"] = "age-v1.3.1-linux-amd64.tar.gz",
			},
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
