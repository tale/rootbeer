// #include "rootbeer_core.h"
#include "rb_rootbeer.h"
#include "rb_plugin.h"

int rb_brew(lua_State *L) {
	// Check if the first argument is a string
	if (!lua_isstring(L, 1)) {
		lua_pushstring(L, "Expected a string as the first argument");
		lua_error(L);
		return 0; // This line will never be reached due to lua_error
	}

	// Setuid to effective user ID since brew cannot run as root
	// and it is user specific.
	const char *user_uid = getenv("SUDO_UID");
	const char *user_gid = getenv("SUDO_GID");
	if (user_uid && user_gid) {
	    uid_t uid = (uid_t)atoi(user_uid);
	    gid_t gid = (gid_t)atoi(user_gid);

	    setgid(gid);
	    setuid(uid);
	}

	const char *brew_name = lua_tostring(L, 1);
	char **args = malloc(3 * sizeof(char *));
	if (args == NULL) {
		lua_pushstring(L, "Memory allocation failed");
		lua_error(L);
		return 0; // This line will never be reached due to lua_error
	}

args[0] = strdup("/opt/homebrew/bin/brew");
	if (args[0] == NULL) {
		free(args);
		lua_pushstring(L, "Memory allocation failed");
		lua_error(L);
		return 0; // This line will never be reached due to lua_error
	}

args[1] = strdup(brew_name);
	if (args[1] == NULL) {
		free(args[0]);
		free(args);
		lua_pushstring(L, "Memory allocation failed");
		lua_error(L);
		return 0; // This line will never be reached due to lua_error
	}

args[2] = NULL; // Null-terminate the array of arguments

	int status = rb_execute_command(NULL, "/opt/homebrew/bin/brew", args);
	if (status != 0) {
		lua_pushstring(L, "Failed to execute brew command");
		lua_error(L);
		return 0; // This line will never be reached due to lua_error
	}

	setuid(0); // Reset to root user after executing brew command
	setgid(0); // Reset to root group after executing brew command

	return 0;
}

static const luaL_Reg functions[] = {
	{"brew", rb_brew},
	{NULL, NULL}
};

RB_PLUGIN(brew, "brew plugin", "1.0.0", functions)
