#include "lua.h"
#include "rootbeer_core.h"

static int rb_lua_return_upvalue(lua_State *L) {
    lua_pushvalue(L, lua_upvalueindex(1));
    return 1;
}

// TODO: Support adding existing modules by coalescing with the existing table
int rb_core_register_module(lua_State *L) {
	const char *modname = luaL_checkstring(L, 1);
	luaL_checktype(L, 2, LUA_TTABLE);
	if (modname == NULL || modname[0] == '\0') {
		luaL_error(L, "Module name cannot be empty");
		return 0;
	}

	// We need to check if "rootbeer.<modname>" already exists

	lua_getglobal(L, "package");
	lua_getfield(L, -1, "preload");

	char fullmodname[256];
	snprintf(fullmodname, sizeof(fullmodname), "rootbeer.%s", modname);

	lua_getfield(L, -1, fullmodname);
	if (!lua_isnil(L, -1)) {
		lua_pop(L, 3); // pop preload table, package table, and existing module
		return luaL_error(L, "Module '%s' already exists", fullmodname);
	}

	// Create a new module table
	lua_pop(L, 1); // pop nil value for existing module

	// preload[fullmodname] = function() return <module table> end
    lua_pushstring(L, fullmodname);
    lua_pushvalue(L, 2); // the module table
    lua_pushcclosure(L, rb_lua_return_upvalue, 1); // closure to return the module table
    lua_settable(L, -3); // preload[fullmodname] = function

    lua_pop(L, 2); // pop preload and package
    return RB_OK;
}
