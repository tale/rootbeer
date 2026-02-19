librootbeer_sources = files(
	'src/api/intermediate.c',
	'src/api/output_buffer.c',
	'src/api/track_gen_file.c',
	'src/api/track_ref_file.c',
	'src/helpers/canon.c',
	'src/helpers/idlist.c',
	'src/helpers/strlist.c',
)

librootbeer = static_library(
	'rootbeer',
	librootbeer_sources,
	dependencies: [dependency('luajit')],
	include_directories: [root_include, internal_include],
)

librootbeer_dep = declare_dependency(
	link_with: librootbeer,
	include_directories: [root_include, internal_include],
)

# rootbeer_sources += [librootbeer]
