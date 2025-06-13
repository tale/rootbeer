librootbeer_sources = files(
	'src/api/track_gen_file.c',
	'src/api/track_ref_file.c',
)

librootbeer = static_library(
	'rootbeer',
	librootbeer_sources,
	dependencies: [dependency('luajit')],
	include_directories: [root_include],
)

librootbeer_dep = declare_dependency(
	link_with: librootbeer,
	include_directories: [root_include],
)

# rootbeer_sources += [librootbeer]
