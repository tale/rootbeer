plugin_dirs = ['rootbeer_core', 'rpm_pkg']

plugin_sources = []
plugin_names = []
plugin_libs = []

foreach d : plugin_dirs
	subdir(d)
	if is_variable('register_plugin_sources')
		plugin_sources += register_plugin_sources
	endif

	if is_variable('register_plugin_name')
		plugin_names += register_plugin_name
	endif

	if is_variable('register_plugin_lib')
		plugin_libs += register_plugin_lib
	endif
endforeach

subdir('gen_plugin_registry')
