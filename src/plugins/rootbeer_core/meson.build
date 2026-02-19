rootbeer_lib = static_library(
	'rootbeer_plugin',
	sources: files(
		'src/core.c',
		'src/data.c',
		'src/emit.c',
		'src/interpolate_table.c',
		'src/link_file.c',
		'src/register_module.c',
		'src/to_json.c',
		'src/write_file.c',
	),
	c_args: ['-DRB_PLUGIN_OVERRIDES_INTERNAL'],
	# This is the core rootbeer plugin, it has access to the real rb_ctx_t
	include_directories: [include_directories('src'), root_include, internal_include],
	dependencies: [dependency('libcjson'), dependency('luajit')],
)

register_plugin_lib = rootbeer_lib
register_plugin_name = '__rootbeer__'
