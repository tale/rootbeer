#include "rootbeer_core.h"
#include <sys/utsname.h>
#include <unistd.h>
#include <stdlib.h>
#include <limits.h>

int rb_core_data(lua_State *L) {
	struct utsname uts;
	uname(&uts);

	char hostname[256];
	gethostname(hostname, sizeof(hostname));

	const char *home = getenv("HOME");
	const char *user = getenv("USER");

	lua_newtable(L);

	lua_pushstring(L, uts.sysname);
	lua_setfield(L, -2, "os");

	lua_pushstring(L, uts.machine);
	lua_setfield(L, -2, "arch");

	lua_pushstring(L, hostname);
	lua_setfield(L, -2, "hostname");

	if (home) {
		lua_pushstring(L, home);
		lua_setfield(L, -2, "home");
	}

	if (user) {
		lua_pushstring(L, user);
		lua_setfield(L, -2, "username");
	}

	return 1;
}
