#include "lua_module.h"
#include "rootbeer.h"

// This is the require hook that we use to gather a list of required
// files so that we can store these in the revision later.
int rb_lua_require_hook(lua_State *L) {
	rb_lua_t *ctx = lua_touserdata(L, lua_upvalueindex(1));
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

	// Is 100 enough? I don't know, but it's a good start.
	ctx->req_filesv = malloc(100 * sizeof(char *));
	ctx->req_filesc = 0;

	lua_pushlightuserdata(ctx->L, ctx);
	lua_pushcclosure(ctx->L, rb_lua_require_hook, 1);
	lua_setglobal(ctx->L, "require");
}
