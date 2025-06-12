if not is_variable('plugin_sources')
	error(
		'%s is expected to be defined with %',
		'`plugin_sources`',
		'a `files()` call of all the plugin source files',
	)
endif

if not is_variable('plugin_names')
	error(
		'%s is expected to be defined with %',
		'`plugin_names`',
		'a `string_array()` call of all the plugin names',
	)
endif

py_script = find_program('gen_plugin_registry.py', required: true)
message('Plugins to register: ' + ', '.join(plugin_names))

plugin_registry_gen = custom_target(
	'gen_plugin_registry',
	output: 'plugin_registry.c',
	command: [py_script, '@OUTPUT@'] + plugin_names,
)

plugin_taped_sources = plugin_sources + [plugin_registry_gen]
