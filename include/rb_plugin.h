/**
 * @file rb_plugin.h
 * @brief Public API for defining plugins within Rootbeer.
 *
 * This header file defines the structure and macros used to create plugins
 * for Rootbeer. Plugins are the core of Rootbeer's functionality, turning
 * it from a glorified Lua interpreter into a powerful tool for managing
 * revisions and applying changes with proper side effects.
 *
 * Plugins are meant to export a set of native functions that can be called
 * from within the Lua environment, allowing for a rich interaction with
 * Rootbeer's core features. Each plugin is defined with a name, description,
 * version, and a set of functions that it provides to the Lua environment.
 *
 * Plugin functions have access to Rootbeer's internals via `librootbeer`,
 * allowing any possible functionality that can tie itself to a revision.
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
 * Manually defining this structure is not recommended, as the plugin
 * system is expecting a statically linked structure that is
 * defined as a global variable in the plugin source file.
 * To define a plugin, use the macro @ref RB_PLUGIN.
 */
typedef struct {
	const char *plugin_name; //!< Registers a `rootbeer.<name>` module in Lua.
	const char *description; //!< Description of the plugin for the CLI.
	const char *version; //!< Version of the plugin, used for CLI and Lua.
	const luaL_Reg *functions; //!< Array of functions to be registered in Lua.
	int (*entrypoint)(lua_State *L); //!< Generates a function for the package.preload table.
} rb_plugin_t;


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
 * @code{.c}
 * #include "rb_plugin.h"
 * const luaL_Reg myplugin_functions[] = {
 *     {"my_function", my_function_impl},
 *     {NULL, NULL} // Sentinel to mark the end of the array
 * };
 *
 * RB_PLUGIN(myplugin, "My Plugin Description", "1.0.0", myplugin_functions);
 * @endcode
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
