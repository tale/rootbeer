#include <lua.h>
#include <lualib.h>
#include <lauxlib.h>
#include <luajit.h>

#include "rootbeer.h"

int test_from_c(lua_State *L) {
	const char *str = lua_tostring(L, 1);
	printf("Hello from C: %s\n", str);
	return 1;
}

int main(const int argc, const char *argv[]) {
	if (argc != 2) {
		printf("Usage: %s <lua file>\n", argv[0]);
		return 1;
	}

	const char *filename = argv[1];
	lua_State *L = luaL_newstate();
	luaL_openlibs(L);
	lua_register(L, "test_from_c", test_from_c);

	if (luaL_dofile(L, filename) != LUA_OK) {
		printf("Error: %s\n", lua_tostring(L, -1));
		lua_pop(L, 1);
	}

	lua_close(L);
	return 0;
}
