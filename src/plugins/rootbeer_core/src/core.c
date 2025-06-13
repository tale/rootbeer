#include "rootbeer_core.h"

int hello_world(lua_State *L) {
	lua_pushstring(L, "Hello, World!");
	return 1;
}

int rb_core_ref_file(lua_State *L) {
	const char *filename = luaL_checkstring(L, 1);
	rb_lua_t *ctx = rb_lua_get_ctx(L);

	int status = rb_track_ref_file(ctx, (char *)filename);
	if (status != 0) {
		return luaL_error(L, "Failed to track file: %s", filename);
	}

	return 0;
}

const luaL_Reg functions[] = {
	{"hello_world", hello_world},
	{"ref_file", rb_core_ref_file},
	{"link_file", rb_core_link_file},
	{"to_json", rb_core_to_json},
	{"write_file", rb_core_write_file},
	{"interpolate_table", rb_core_interpolate_table},
	{NULL, NULL}
};

RB_PLUGIN(__rootbeer__, "a bobr rootbeer", "1.0.0", functions)
