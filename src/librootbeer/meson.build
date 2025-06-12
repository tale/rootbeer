librootbeer_sources = files('src/api/track_file.c')

librootbeer = static_library(
	'rootbeer',
	librootbeer_sources,
	dependencies: [dependency('luajit')],
	include_directories: [
		include_directories('include'),
		include_directories('../../include'),
	],
)

librootbeer_dep = declare_dependency(
	link_with: librootbeer,
	include_directories: [
		include_directories('include'),
		include_directories('../../include'),
	],
)

# rootbeer_sources += [librootbeer]
