#include "rootbeer.h"
#include "lua_module.h"

void rb_lua_load_lib(rb_lua_t *ctx) {
	lua_getglobal(ctx->L, "package");
	lua_getfield(ctx->L, -1, "path");

	const char *curr_path = lua_tostring(ctx->L, -1);
	lua_pop(ctx->L, 1);

	// In non-debug builds, we use the extracted rootbeer Lua libraries.
	// In debug, we just use the lua/ directory in the git source-tree.
	// It's safe to assume that is PWD in debug builds.
	char system_path[PATH_MAX];
#ifndef DEBUG
	snprintf(
		system_path, sizeof(system_path),
		"%s/?.lua;%s/?/init.lua", LUA_LIB, LUA_LIB
	);
#else
	char *pwd = getcwd(NULL, 0);
	printf("DEBUG: Using %s/lua/ for Lua libraries\n", pwd);
	snprintf(
		system_path, sizeof(system_path),
		"%s/lua/?.lua;%s/lua/?/init.lua", pwd, pwd
	);
#endif

	char rel_path[PATH_MAX];
	snprintf(
		rel_path, sizeof(rel_path),
		"%s/lua/?.lua;%s/lua/?/init.lua", ctx->config_root, ctx->config_root
	);

	size_t new_path_len = strlen(curr_path)
		+ strlen(system_path)
		+ strlen(rel_path)
		+ 3; // 2 semicolons and null terminator

	char *new_path = malloc(new_path_len);
	if (new_path == NULL) {
		rb_fatal("Failed to allocate space for modified lua path");
	}

	snprintf(new_path, new_path_len, "%s;%s;%s", curr_path, system_path, rel_path);
	lua_pushstring(ctx->L, new_path);
	lua_setfield(ctx->L, -2, "path");
	lua_pop(ctx->L, 1);
	free(new_path);
}
