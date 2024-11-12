#include <lua.h>
#include <lualib.h>
#include <lauxlib.h>
#include <luajit.h>

#include "rootbeer.h"
#include "lua_module.h"

int main(const int argc, const char *argv[]) {
	if (argc != 2) {
		printf("Usage: %s <lua file>\n", argv[0]);
		return 1;
	}

	const char *filename = argv[1];
	lua_State *L = luaL_newstate();
	luaL_openlibs(L);
	rb_lua_create_module(L);

	if (luaL_dofile(L, filename) != LUA_OK) {
		printf("Error: %s\n", lua_tostring(L, -1));
		lua_pop(L, 1);
	}

	lua_close(L);
	return 0;
}
