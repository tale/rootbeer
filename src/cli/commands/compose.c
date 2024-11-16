#include "cli_module.h"
#include "lua_module.h"
#include "store_module.h"

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
				config_file = optarg;
				break;
			case '?':
				return 1;
		}
	}

	// Check access permissions on the config file
	if (access(config_file, F_OK | R_OK) != 0) {
		fprintf(stderr, "error: could not access config file\n");
		return 1;
	}

	rb_lua_t ctx;
	ctx.config_file = config_file;
	rb_lua_setup_context(&ctx);

	int status = luaL_dofile(ctx.L, ctx.config_file);
	if (status != LUA_OK) {
		fprintf(stderr, "Failed to execute lua configuration:\n");
		fprintf(stderr, "%s\n", lua_tostring(ctx.L, -1));
		lua_pop(ctx.L, 1);
		return 1;
	}

	rb_store_dump_revision(&ctx);
	printf("Revision composed successfully\n");
	lua_close(ctx.L);
	return 0;
}
