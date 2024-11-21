#include "lua_module.h"

// We pack all of the lua functions here into a single struct
// so that they can all be loaded in as a module and called in lua code

static const struct luaL_Reg functions[] = {
	{"debug_test", rb_lua_debug_test},
	{"ref_file", rb_lua_ref_file},
	{NULL, NULL}
};

// Virtual package entrypoint for the package.preload table
int lua_mod_entrypoint(lua_State *L) {
	luaL_newlib(L, functions);
	return 1;
}

void rb_lua_register_module(lua_State *L) {
	lua_pushcfunction(L, lua_mod_entrypoint);
	lua_setfield(L, LUA_REGISTRYINDEX, "rootbeer");

	// Add it to package.preload now that it's loaded
	lua_getglobal(L, "package");
	lua_getfield(L, -1, "preload");
	lua_pushcfunction(L, lua_mod_entrypoint);
	lua_setfield(L, -2, "rootbeer");
	lua_pop(L, 2);
}

rb_lua_t *rb_lua_get_ctx(lua_State *L) {
	lua_pushlightuserdata(L, (void *)rb_lua_get_ctx);
    lua_gettable(L, LUA_REGISTRYINDEX);
	rb_lua_t *ctx = lua_touserdata(L, -1);

	if (ctx == NULL) {
		luaL_error(L, "context is null");
		return NULL;
	}

	return ctx;
}
