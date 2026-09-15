return {
	schema = 2,
	name = "age",
	description = "File encryption with explicit recipients",
	default_version = "1.3.1",
	homepage = "https://age-encryption.org/",
	systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
	inputs = {
		prebuilt = {
			github = "FiloSottile/age",
			tag = "v1.3.1",
			assets = {
				["aarch64-linux"] = "age-v1.3.1-linux-arm64.tar.gz",
				["aarch64-macos"] = "age-v1.3.1-darwin-arm64.tar.gz",
				["x86_64-linux"] = "age-v1.3.1-linux-amd64.tar.gz",
			},
		},
	},
	outputs = {
		bins = { "age", "age-keygen" },
		checks = { { "age", "--version" }, { "age-keygen", "--version" } },
	},
	versions = {
		["1.3.1"] = {
			revision = 3,
			systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
			inputs = {
				prebuilt = {
					github = "FiloSottile/age",
					tag = "v1.3.1",
					assets = {
						["x86_64-linux"] = "age-v1.3.1-linux-amd64.tar.gz",
						["aarch64-macos"] = "age-v1.3.1-darwin-arm64.tar.gz",
						["aarch64-linux"] = "age-v1.3.1-linux-arm64.tar.gz",
					},
				},
			},
			outputs = {
				bins = { "age", "age-keygen" },
				checks = {
					{ "age", "--version" },
					{ "age-keygen", "--version" },
				},
			},
		},
	},
}
