#include "rootbeer_core.h"

// Takes in a table of values and a lua function to return the interpolated value
int rb_core_interpolate_table(lua_State *L) {
	luaL_checktype(L, 1, LUA_TTABLE);
	luaL_checktype(L, 2, LUA_TFUNCTION);

	lua_pushvalue(L, 2);
	lua_pushvalue(L, 1); // Pass the table to the function

	if (lua_pcall(L, 1, 1, 0) != LUA_OK) {
		const char *error = lua_tostring(L, -1);
		lua_pop(L, 1); // Remove the error message from the stack
		luaL_error(L, "Error in interpolation function: %s", error);
		return 0;
	}

	if (!lua_isstring(L, -1)) {
		luaL_error(L, "Interpolation function must return a string");
		return 0;
	}

	// Return the string result
	const char *result = lua_tostring(L, -1);
	lua_pop(L, 1); // Remove the result from the stack
	lua_pushstring(L, result);
	return 1; // Return the interpolated string
}
