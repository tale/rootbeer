homebrew_lib = static_library(
	'homebrew_plugin',
	sources: files(
		'src/core.c',
	),
	include_directories: [include_directories('src'), root_include],
	dependencies: [dependency('luajit')],
)

register_plugin_lib = homebrew_lib
register_plugin_name = 'brew'
