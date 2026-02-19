plugin_dirs = ['rootbeer_core']
plugin_libs = []

foreach d : plugin_dirs
	subdir(d)
	if is_variable('register_plugin_lib')
		plugin_libs += register_plugin_lib
	endif
endforeach

plugin_taped_sources = files('plugin_registry.c')
