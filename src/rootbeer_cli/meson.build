cli_sources = files(
	'src/cli_cmds/apply.c',
	'src/cli_cmds/store.c',
)

subdir('gen_cmd_array')
src_files = cli_taped_sources + plugin_taped_sources + files(
	'src/cli_tool/command.c',
	'src/cli_tool/parse.c',

	'src/lua_config/functions/debug_test.c',
	'src/lua_config/functions/ref_file.c',
	'src/lua_config/lua_init.c',
	'src/lua_config/lua_module.c',

	'src/main.c',

	'src/rev_store/create.c',
	'src/rev_store/current.c',
	'src/rev_store/manage.c',
	'src/rev_store/read.c',
	'src/rev_store/view.c',

	'src/util/fs.c',
	'src/util/log.c',
)

executable(
	'rb',
	src_files,
	dependencies: [dependency('luajit'), librootbeer_dep],
	link_with: plugin_libs,
	include_directories: [
		include_directories('include'),
		include_directories('../../include'),
	],
)
