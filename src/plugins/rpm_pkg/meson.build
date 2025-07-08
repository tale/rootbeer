libsolv_dep = dependency('libsolv', required: true, native: true)

rpm_pkg_lib = static_library(
	'rpm_pkg_plugin',
	sources: files('src/plugin.c', 'src/query.c', 'src/solv/repo_loader.c'),
	include_directories: [include_directories('src'), root_include],
	dependencies: [dependency('luajit'), libsolv_dep],
)

register_plugin_lib = rpm_pkg_lib
register_plugin_name = 'rpm_pkg'
