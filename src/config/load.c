#include "lua_module.h"

// Checks all the appropriate locations for the rootbeer config folder
// This includes XDG_CONFIG_HOME and XDG_CONFIG_DIRS falling back to ~
char *rb_lua_get_config_root() {
	char *config_root = getenv("XDG_CONFIG_HOME");
	if (config_root != NULL) {
		char *path = malloc(strlen(config_root) + strlen("/rootbeer") + 1);
		strcpy(path, config_root);
		strcat(path, "/rootbeer");

		if (access(path, F_OK | R_OK) == 0) {
			return path;
		}
	}

	char *config_dirs = getenv("XDG_CONFIG_DIRS");
	if (config_dirs != NULL) {
		char *dir = strtok(config_dirs, ":");
		while (dir != NULL) {
			char *path = malloc(strlen(dir) + strlen("/rootbeer") + 1);
			strcpy(path, dir);
			strcat(path, "/rootbeer");

			if (access(path, F_OK | R_OK) == 0) {
				return path;
			}

			free(path);
			dir = strtok(NULL, ":");
		}
	}

	char *home = getenv("HOME");
	if (home != NULL) {
		char *path = malloc(strlen(home) + strlen("/.config/rootbeer") + 1);
		strcpy(path, home);
		strcat(path, "/.config/rootbeer");

		if (access(path, F_OK | R_OK) == 0) {
			return path;
		}

		free(path);
	}

	// There is nowhere else we can look, so we return NULL
	return NULL;
}

// Multi-purpose function to look for a lua-file in all the known locations
char *rb_lua_find_file(lua_State *L) {
	char *config_root = rb_lua_get_config_root();
	if (config_root == NULL) {
		return NULL;
	}

	char *path = malloc(strlen(config_root) + strlen("/init.lua") + 1);
	if (path == NULL) {
		printf("error: failed to allocate path for init.lua\n");
		free(config_root);
		return NULL;
	}

	strcpy(path, config_root);
	strcat(path, "/init.lua");

	if (access(path, F_OK | R_OK) != 0) {
		printf("error: could not find init.lua in rootbeer config directory\n");
		free(path);
		free(config_root);
		return NULL;
	}

	// Add the config directory to the lua path so that we can require other
	// files that are relative to the config directory for configurations
	lua_getglobal(L, "package");
	lua_getfield(L, -1, "path");
	const char *lua_path = lua_tostring(L, -1);
	char *new_path = malloc(
		strlen(lua_path) +
		strlen(";") +
		strlen(config_root) +
		strlen("/?.lua") + 1
	);

	if (new_path == NULL) {
		printf("error: failed to allocate new lua path\n");
		free(path);
		free(config_root);
		return NULL;
	}

	strcpy(new_path, lua_path);
	strcat(new_path, ";");
	strcat(new_path, config_root);
	strcat(new_path, "/?.lua");

	lua_pop(L, 1); // Remove the old path
	lua_pushstring(L, new_path);
	lua_setfield(L, -2, "path");
	lua_pop(L, 1); // Remove the package table
	
	free(new_path);
	free(config_root);
	return path;
}

// Used to set and enforce some resource limits on the lua VM
void rb_lua_resource_hook(lua_State *L, lua_Debug *ar) {
	(void)ar;
	luaL_error(L, "resource limit exceeded");
	printf("error: resource limit exceeded\n");
}

// Given a new lua state, set up the entire luaJIT interpreter
// This loads all of our custom modules, sets up the package path, and
// loads the init.lua file from the rootbeer config directory
// 
// This also handles checking if the argv filename is a valid lua file
// since we support supplying a path to a lua file directly to the program
//
// Caveat: The lua package.path PWD is set to the directory of the lua file
// that is being executed, NOT THE CURRENT WORKING DIRECTORY.
int rb_lua_create_vm_sandbox(lua_State *L, const char *filename) {
	assert(L != NULL);

	// We allow all the libraries because this is a system configuration tool
	// it's not like the lua file is running as root man.
	
	luaL_openlibs(L);
	rb_lua_create_module(L);

	// Limit the instruction count to 1 million
	lua_sethook(L, rb_lua_resource_hook, LUA_MASKCOUNT, 1000000);
	char *init_lua = NULL;

	if (filename != NULL) {
		if (access(filename, F_OK | R_OK) != 0) {
			printf("error: could not access file %s\n", filename);
			return 1;
		}

		init_lua = malloc(strlen(filename) + 1);
		if (init_lua == NULL) {
			printf("error: failed to allocate memory for filename\n");
			return 1;
		}

		strcpy(init_lua, filename);

		// Set the package.path to the directory of the lua file
		char *dir = dirname(init_lua);
		lua_getglobal(L, "package");
		lua_pushstring(L, dir);
		lua_setfield(L, -2, "path");
		lua_pop(L, 1);
	} else {
		init_lua = rb_lua_find_file(L);
	}

	if (init_lua == NULL) {
		// TODO: this error message is not very helpful
		printf("error: could not find init.lua\n");
		exit(1);
	}

	int status = luaL_dofile(L, init_lua);
	if (status != LUA_OK) {
		printf("error: %s\n", lua_tostring(L, -1));
		lua_pop(L, 1);
		free(init_lua);
		return 1;
	}

	free(init_lua);
	return 0;
}
