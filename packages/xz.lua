return {
	name = "xz",
	description = "Compress and decompress XZ streams",
	homepage = "https://tukaani.org/xz/",
	default_version = "5.8.3",
	versions = {
		["5.8.3"] = {
			revision = 1,
			build = {
				backend = "autotools",
				url = "https://github.com/tukaani-project/xz/releases/download/v5.8.3/xz-5.8.3.tar.gz",
				sha256 = "3d3a1b973af218114f4f889bbaa2f4c037deaae0c8e815eec381c3d546b974a0",
				archive = "tar.gz",
				strip_prefix = "xz-5.8.3",
				configure = {
					"--disable-shared",
					"--enable-static",
					"--disable-nls",
					"--disable-scripts",
					"--disable-doc",
				},
			},
			systems = {
				"aarch64-macos",
				"x86_64-macos",
				"aarch64-linux",
				"x86_64-linux",
			},
			bins = { "xz", "xzdec", "lzmadec", "lzmainfo" },
			checks = {
				{ "xz", "--version" },
				{ "xzdec", "--version" },
				{ "lzmadec", "--version" },
				{ "lzmainfo", "--version" },
			},
		},
	},
}
