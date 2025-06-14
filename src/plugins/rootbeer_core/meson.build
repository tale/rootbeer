rootbeer_lib = static_library(
	'rootbeer_plugin',
	sources: files(
		'src/core.c',
		'src/interpolate_table.c',
		'src/link_file.c',
		'src/register_module.c',
		'src/to_json.c',
		'src/write_file.c',
	),
	include_directories: [include_directories('src'), root_include],
	dependencies: [dependency('libcjson'), dependency('luajit')],
)

register_plugin_lib = rootbeer_lib
register_plugin_name = '__rootbeer__'
