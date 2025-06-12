plugin_dirs = ['rootbeer_core']

plugin_sources = []
plugin_names = []

foreach d : plugin_dirs
	subdir(d)
	plugin_sources += register_plugin_sources
	plugin_names += register_plugin_name
endforeach

subdir('gen_plugin_registry')
