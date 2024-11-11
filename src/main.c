#include <lua.h>
#include <lualib.h>
#include <lauxlib.h>
#include <luajit.h>

int test_from_c(lua_State *L) {
	const char *str = lua_tostring(L, 1);
	printf("Hello from C: %s\n", str);
	return 1;
}

int main(void) {
	lua_State *L = luaL_newstate();
	luaL_openlibs(L);
	lua_register(L, "test_from_c", test_from_c);

	const char *code = "test_from_c('Architect')";
	if (luaL_dostring(L, code) != LUA_OK) {
		printf("Error: %s\n", lua_tostring(L, -1));
		lua_pop(L, 1);
	}

	lua_close(L);
	return 0;
}
