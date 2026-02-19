cli_sources = files(
	'plugin_registry.c',

	'cli_cmds/apply.c',
	'cli_cmds/init.c',
	'cli_cmds/store.c',
	'cli_cmds/cmd_registry.c',

	'cli_tool/command.c',
	'cli_tool/parse.c',

	'lua_config/lua_init.c',
	'lua_config/rb_ctx_state.c',

	'main.c',

	'rev_store/create.c',
	'rev_store/current.c',
	'rev_store/manage.c',
	'rev_store/read.c',
	'rev_store/view.c',

	'util/fs.c',
	'util/log.c',
)

executable(
	'rb',
	cli_sources,
	dependencies: [dependency('luajit'), core_dep],
	include_directories: [include_directories('include')],
)
