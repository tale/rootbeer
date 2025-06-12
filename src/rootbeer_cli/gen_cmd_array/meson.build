if not is_variable('cli_sources')
	error(
		'%s is expected to be defined with %',
		'`cli_sources`',
		'a `files()` call of all the CLI command source files',
	)
endif

py_script = find_program('gen_cmd_array.py', required: true)
cmd_array_gen = custom_target(
	'gen_cmd_array',
	input: cli_sources,
	output: 'cmd_array.c',
	command: [
		py_script,
		'@OUTPUT@',
		'@INPUT@',
	],
)

cli_taped_sources = cli_sources + [cmd_array_gen]
