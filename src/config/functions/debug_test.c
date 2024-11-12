#include "lua_module.h"
#include <stdio.h>

int rb_lua_debug_test(lua_State *L) {
	const char *str = lua_tostring(L, 1);
	printf("DEBUG TEST INVOKED WITH: %s\n", str);
	return 1;
}
