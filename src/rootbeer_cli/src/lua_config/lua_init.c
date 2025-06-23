#include "lua_init.h"
#include "rb_ctx.h"
#include "rootbeer.h"
#include "rb_plugin.h"
#include <lauxlib.h>
#include <lualib.h>

// This is linked in by Meson when compiling the plugins.
extern const rb_plugin_t *rb_plugins[];

int lua_runtime_require_hook(lua_State *L) {
	const char *modname = luaL_checkstring(L, 1);
	if (modname == NULL) {
		return luaL_error(L, "recieved an invalid module name");
	}

	lua_getglobal(L, "require_orig");
	lua_pushstring(L, modname);
	if (lua_pcall(L, 1, 1, 0) != LUA_OK) {
		return luaL_error(L, "error loading %s: %s", modname, lua_tostring(L, -1));
	}

	// Lua modules are dot separate to indicate directories, meaning we need
	// to do the work to convert them back into slashes for the filesystem.
	char modpath[strlen(modname) + 1];
	strncpy(modpath, modname, strlen(modname) + 1);
	for (size_t i = 0; i < strlen(modpath); i++) {
		if (modpath[i] == '.') {
			modpath[i] = '/';
		}
	}

	// The full path on disk falls in /lua/<modname>.lua
	size_t modname_len = strlen(modpath) + strlen("/lua/") + strlen(".lua") + 1;
	char filepath[modname_len];
	snprintf(filepath, modname_len, "/lua/%s.lua", modpath);

	// We also used to previously check access() here, but we know we don't
	// have to do that because the lua_pcall() would've failed earlier.
	rb_ctx_t *ctx = rb_ctx_from_lua(L);
	ctx->lua_files[ctx->lua_files_count++] = strdup(filepath);
	return 1;
}

int lua_runtime_init(lua_State *L, const char *entry_file) {
	assert(entry_file != NULL);
	assert(L != NULL);

	// Add our baked-in Lua libraries to the path.
	luaL_openlibs(L);
	lua_getglobal(L, "package");
	lua_getfield(L, -1, "path");
	const char *lpath = lua_tostring(L, -1);
	lua_pop(L, 1);

	// In non-debug builds, we use the extracted rootbeer Lua libraries.
	// In debug, we just use the lua/ directory in the git source-tree.
	// In this case, we make the assumption that the CWD is the source root.
	char system_path[PATH_MAX];
#ifndef DEBUG
	snprintf(
		system_path, sizeof(system_path),
		"%s/?.lua;%s/?/init.lua", LUA_LIB, LUA_LIB
	);
#else
	char *pwd = getcwd(NULL, 0);
	if (pwd == NULL) {
		rb_fatal("Could not get current working directory");
	}

	printf("DEBUG: Using %s/lua/ for Lua libraries\n", pwd);
	snprintf(
		system_path, sizeof(system_path),
		"%s/lua/?.lua;%s/lua/?/init.lua", pwd, pwd
	);

	free(pwd);
#endif

	size_t new_lpath_len = strlen(lpath)
		+ strlen(system_path)
		+ strlen("/lua/?.lua")
		+ strlen("/lua/?/init.lua")
		+ 3; // 2 semicolons and null terminator

	char *new_lpath = malloc(new_lpath_len);
	if (new_lpath == NULL) {
		rb_fatal("Failed to allocate space for modified lua path");
	}

	snprintf(new_lpath, new_lpath_len, "%s;%s/lua/?.lua;%s/lua/?/init.lua",
		lpath, LUA_LIB, LUA_LIB);
	lua_pushstring(L, new_lpath);
	lua_setfield(L, -2, "path");
	lua_pop(L, 1);
	free(new_lpath);

	// Setup our hook to the require function allowing us to track
	// which files are required by the Lua scripts. Keep in mind that there are
	// other ways to load Lua files, such as `dofile` or `loadfile`, but we
	// only choose to track the `require` function for now.
	//
	// TODO: Reconsider this if we find that we need to track more.
	lua_getglobal(L, "require");
	lua_setglobal(L, "require_orig");
	lua_pushcclosure(L, lua_runtime_require_hook, 1);
	lua_setglobal(L, "require");

	// Load our plugins into the Lua environment.
	for (const rb_plugin_t **p = rb_plugins; *p != NULL; p++) {
		const rb_plugin_t *plugin = *p;
		// Plugin names are done as rootbeer.<plugin_name>
		// Which means we need to snprintf the name
		char plugin_name[64];
		snprintf(plugin_name, sizeof(plugin_name), "rootbeer.%s", plugin->plugin_name);

		// Handle the special case the plugin is called "__rootbeer__"
		// which is the rootbeer plugin itself and needs to be on the root.
		if (strcmp(plugin->plugin_name, "__rootbeer__") == 0) {
			snprintf(plugin_name, sizeof(plugin_name), "rootbeer");
		}

		printf("Loading plugin: %s\n", plugin_name);
		lua_pushcfunction(L, plugin->entrypoint);
		lua_setfield(L, LUA_REGISTRYINDEX, plugin_name);

		// Add to package.preload
		lua_getglobal(L, "package");
		lua_getfield(L, -1, "preload");
		lua_pushcfunction(L, plugin->entrypoint);
		lua_setfield(L, -2, plugin_name);
		lua_pop(L, 2);
	}

	return 0;
}

int lua_register_context(lua_State *L, rb_ctx_t *ctx) {
	lua_pushlightuserdata(L, (void *)rb_ctx_from_lua);
	lua_pushlightuserdata(L, (void *)ctx);
	lua_settable(L, LUA_REGISTRYINDEX);
	return 0;
}
