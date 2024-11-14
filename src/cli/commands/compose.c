#include "cli_module.h"
#include "lua_module.h"

// The compose command is where we tell rootbeer to interpret the lua
// configuration and create a new system configuration revision.
//
// Flags:
// -c, --config: The path to the lua configuration file (fallback to defaults)
int rb_cli_compose(const int argc, const char *argv[]) {
	// Check for our flags
	int opt;
	int opt_index = 0;

	struct option long_opts[] = {
		{"config", required_argument, 0, 'c'},
		{0, 0, 0, 0}
	};

	char *config_file = NULL;
	while ((opt = getopt_long(
		argc,
		(char *const *)argv,
		"c:",
		long_opts,
		&opt_index
	)) != -1) {
		switch (opt) {
			case 'c':
				printf("config: %s\n", optarg);
				config_file = optarg;
				break;
			case '?':
				printf("unknown flag: %c\n", opt);
				break;
		}
	}

	lua_State *L = luaL_newstate();
	int status = rb_lua_create_vm_sandbox(L, config_file);
	if (status != 0) {
		printf("error: could not create vm sandbox\n");
		lua_close(L);
		return 1;
	}

	lua_close(L);
	return 0;
}
