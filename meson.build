project(
	'rootbeer',
	'c',
	version: '0.0.1',
	license: 'MIT',
	default_options: [
		'c_std=gnu99',
		'buildtype=debugoptimized',
		'warning_level=2',
	],
)
root_include = include_directories('include')
internal_include = include_directories('src/internal_include')

if get_option('buildtype').startswith('debug')
	add_project_arguments('-DDEBUG', language: 'c')
endif

subdir('src/librootbeer')
subdir('src/plugins')
subdir('src/rootbeer_cli')
