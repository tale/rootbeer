#include "lua_module.h"
#include "rootbeer.h"
#include "rb_plugin.h"

// This is linked in by Meson when compiling the plugins.
extern const rb_plugin_t *rb_plugins[];

// This is the require hook that we use to gather a list of required
// files so that we can store these in the revision later.
int rb_lua_require_hook(lua_State *L) {
	rb_lua_t *ctx = rb_lua_get_ctx(L);
	const char *modname = luaL_checkstring(L, 1);

	// Call the old require function to actually load the module
	lua_getglobal(L, "old_require");
	lua_pushstring(L, modname);
	if (lua_pcall(L, 1, 1, 0) != LUA_OK) {
		fprintf(stderr, "error: %s\n", lua_tostring(L, -1));
		return luaL_error(L, "error loading module");
	}

	// Lua 5.1 is slightly unintelligent about how it handles require
	// so we need to check if the loaded module exists as a template in
	// `lua/<module>.lua` from the ctx->config_root directory.

	// We also need to replace any dots in the module name with slashes
	// since lua uses dots to separate modules.
	char modname_copy[strlen(modname) + 1];
	strncpy(modname_copy, modname, strlen(modname) + 1);
	for (int i = 0; i < strlen(modname_copy); i++) {
		if (modname_copy[i] == '.') {
			modname_copy[i] = '/';
		}
	}

	size_t f_len = strlen(ctx->config_root)
		+ strlen("/lua/")
		+ strlen(modname_copy)
		+ strlen(".lua") + 1;

	char filename[f_len];
	sprintf(filename, "%s/lua/%s.lua", ctx->config_root, modname_copy);
	if (access(filename, F_OK | R_OK) == 0) {
		ctx->req_filesv[ctx->req_filesc++] = strdup(filename);
	}

	return 1;
}

// Context is stack allocated and already given some information
// about the loading requirements. We need to initialize the lua
// environment and load the config file.
//
// Since rootbeer sets up your dotfiles, the path to the config
// is always required to be passed into the command line.
void rb_lua_setup_context(rb_lua_t *ctx) {
	// Checked beforehand by the caller
	assert(ctx->config_file != NULL);
	ctx->config_root = strdup(dirname(ctx->config_file));

	// Create a new lua state
	ctx->L = luaL_newstate();
	if (ctx->L == NULL) {
		rb_fatal("Could not create lua state");
	}

	// Allow all libraries since this is a *host* configuration tool.
	// There aren't really any security implications here.
	luaL_openlibs(ctx->L);
	rb_lua_register_module(ctx->L);

	// Add the root/lua folder to the package path for custom requires
	lua_getglobal(ctx->L, "package");
	lua_getfield(ctx->L, -1, "path");
	const char *lua_path = lua_tostring(ctx->L, -1);
	char *new_path = malloc(
		strlen(lua_path) +
		strlen(";") +
		strlen(ctx->config_root) +
		strlen("/lua/?.lua") + 1
	);

	if (new_path == NULL) {
		rb_fatal("Failed to allocate space for modified lua path");
	}

	sprintf(new_path, "%s;%s/lua/?.lua", lua_path, ctx->config_root);
	lua_pop(ctx->L, 1); // Remove the old path
	lua_pushstring(ctx->L, new_path);
	lua_setfield(ctx->L, -2, "path");
	lua_pop(ctx->L, 1); // Remove the package table
	free(new_path);

	// Hook the require function to gather a list of required files
	// so that we can store these in the revision later.
	lua_getglobal(ctx->L, "require");
	lua_setglobal(ctx->L, "old_require");

	ctx->req_filesv = malloc(LUAFILES_MAX * sizeof(char *));
	ctx->req_filesc = 0;

	ctx->ref_filesv = malloc(REFFILES_MAX * sizeof(char *));
	ctx->ref_filesc = 0;

	// Use the retrieval function as the ID for the context
	lua_pushlightuserdata(ctx->L, (void *)rb_lua_get_ctx);
	lua_pushlightuserdata(ctx->L, (void *)ctx);
	lua_settable(ctx->L, LUA_REGISTRYINDEX);

	lua_pushcclosure(ctx->L, rb_lua_require_hook, 1);
	lua_setglobal(ctx->L, "require");

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
		lua_pushcfunction(ctx->L, plugin->entrypoint);
		lua_setfield(ctx->L, LUA_REGISTRYINDEX, plugin_name);

		// Add to package.preload
		lua_getglobal(ctx->L, "package");
		lua_getfield(ctx->L, -1, "preload");
		lua_pushcfunction(ctx->L, plugin->entrypoint);
		lua_setfield(ctx->L, -2, plugin_name);
		lua_pop(ctx->L, 2);
	}
}
