#include "lua.h"
#include "rb_plugin.h"
#include "rb_rootbeer.h"

int hello_world(lua_State *L) {
	lua_pushstring(L, "Hello, World!");
	return 1;
}

int ref_file(lua_State *L) {
	const char *filename = luaL_checkstring(L, 1);
	rb_lua_t *ctx = rb_lua_get_ctx(L);

	int status = rb_track_file(ctx, (char *)filename);
	if (status != 0) {
		return luaL_error(L, "Failed to track file: %s", filename);
	}

	return 0;
}

const luaL_Reg functions[] = {
	{"hello_world", hello_world},
	{"ref_file", ref_file},
	{NULL, NULL}
};

RB_PLUGIN(__rootbeer__, "a bobr rootbeer", "1.0.0", functions)
