#include "rb_plugin.h"
#include "rpm_pkg.h"
#include <stdlib.h>
#include <string.h>

static int with_pkgs(lua_State *L) {
	if (!lua_istable(L, 1)) {
		lua_pushstring(L, "Expected a table of package names as the first argument");
		lua_error(L);
		return 0;
	}

	// Convert into char ** array
	size_t len = lua_objlen(L, 1);
	char **pkgs = malloc((len + 1) * sizeof(char *));
	if (pkgs == NULL) {
		lua_pushstring(L, "Memory allocation failed");
		lua_error(L);
		return 0;
	}

	for (size_t i = 0; i < len; i++) {
		lua_rawgeti(L, 1, i + 1); // Lua is 1-indexed
		if (!lua_isstring(L, -1)) {
			free(pkgs);
			lua_pushstring(L, "Expected all elements in the table to be strings");
			lua_error(L);
			return 0;
		}

		pkgs[i] = strdup(lua_tostring(L, -1));
		lua_pop(L, 1); // Pop the string from the stack
		printf("Package %zu: %s\n", i + 1, pkgs[i]);
	}

	query_dnf_packages(pkgs, len);
	return 0;
}

static const luaL_Reg functions[] = {
	{"with_pkgs", with_pkgs},
	{NULL, NULL}
};

RB_PLUGIN(
	rpm_pkg,
	"Deterministically manage packaging on RPM-based systems",
	"0.0.1",
	functions
)
