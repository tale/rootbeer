core_sources = files(
	'api/intermediate.c',
	'api/output_buffer.c',
	'api/track_gen_file.c',
	'api/track_ref_file.c',
	'helpers/canon.c',
	'helpers/idlist.c',
	'helpers/strlist.c',
	'lua/core.c',
	'lua/data.c',
	'lua/emit.c',
	'lua/interpolate_table.c',
	'lua/link_file.c',
	'lua/register_module.c',
	'lua/to_json.c',
	'lua/write_file.c',
)

core_include = include_directories('include')

rootbeer_core = static_library(
	'rootbeer_core',
	core_sources,
	c_args: ['-DRB_PLUGIN_OVERRIDES_INTERNAL'],
	include_directories: [core_include, root_include],
	dependencies: [dependency('libcjson'), dependency('luajit')],
)

core_dep = declare_dependency(
	link_with: rootbeer_core,
	include_directories: [root_include, core_include],
)
