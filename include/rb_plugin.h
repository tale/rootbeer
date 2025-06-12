#ifndef RB_PLUGIN_H
#define RB_PLUGIN_H

#include <lauxlib.h>

/// Defines the structure of a plugin in Rootbeer.
typedef struct {
	const char *plugin_name; ///< Registers a `rootbeer.<name>` module in Lua.
	const char *description; ///< Description of the plugin for the CLI.
	const char *version; ///< Version of the plugin, used for CLI and Lua.
	const luaL_Reg *functions; ///< Array of functions to be registered in Lua.
	int (*entrypoint)(lua_State *L);
} rb_plugin_t;


/// Quick macro to define a plugin in Rootbeer.
/// This is the recommended way to define plugins, as it ensures
/// the correct structure and initialization.
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
