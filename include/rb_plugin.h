/**
 * @file rb_plugin.h
 *
 * This header defines the structure and macros for Rootbeer plugins.
 * Using the @ref RB_PLUGIN macro, developers can easily create plugins
 * for Rootbeer, without having to deal with boilerplate code.
 * See \ghdir{src/plugins/rootbeer_core} for an example of a plugin.
 */
#ifndef RB_PLUGIN_H
#define RB_PLUGIN_H

#include <lauxlib.h>

/**
 * The internal structure for a Rootbeer plugin.
 * This structure defines a plugin's metadata and the functions
 * it provides to the Lua environment. Internally, the CLI will parse
 * this structure to make the plugin available in the Lua environment.
 *
 * Instead of manually defining this, use @ref RB_PLUGIN to define a plugin.
 */
typedef struct {
	const char *plugin_name; //!< Registers a `rootbeer.<name>` module in Lua.
	const char *description; //!< Description of the plugin for the CLI.
	const char *version; //!< Version of the plugin, used for CLI and Lua.
	const luaL_Reg *functions; //!< Array of functions to be registered in Lua.
	int (*entrypoint)(lua_State *L); //!< Generates a function for the package.preload table.
} rb_plugin_t;


/**
 * X-macro list of all registered plugins.
 * To add a new plugin, add a X(name) entry here where `name` matches
 * the identifier used in the RB_PLUGIN() macro in the plugin source.
 */
#define RB_PLUGINS(X) \
	X(__rootbeer__)

/**
 * @def RB_PLUGIN
 * Macro to define a Rootbeer plugin.
 * It will automatically generate the necessary entrypoint function
 * and globals so that the rootbeer CLI can recognize the plugin and load it.
 *
 * This macro simplifies the process of defining a plugin by automatically
 * generating the necessary entrypoint function and populating the plugin
 * structure with the provided name, description, version, and functions.
 *
 * @param name The name of the plugin, used in Lua as `rootbeer.<name>`.
 * @param desc A short description of the plugin for CLI help.
 * @param ver The version of the plugin, used for CLI and Lua.
 * @param f An array of functions to be registered in Lua.
 */
#define RB_PLUGIN(name, desc, ver, f) \
	int lua_mod_entrypoint_##name(lua_State *L) { \
		luaL_newlib(L, f); \
	 	return 1; \
	} \
    const rb_plugin_t rb_plugin_##name = { \
        .plugin_name = #name, \
        .description = desc, \
        .version = ver, \
        .functions = f, \
        .entrypoint = lua_mod_entrypoint_##name \
    }; \


#endif
