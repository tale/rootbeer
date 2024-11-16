#include "cli_module.h"
#include "lua_module.h"
#include "store_module.h"

// The apply command is where we tell rootbeer to interpret the lua
// configuration and create a new system configuration revision.
int rb_cli_apply(const int argc, const char *argv[]) {
	// Check for sudo permissions
	if (geteuid() != 0) {
		fprintf(stderr, "error: rootbeer apply must be run as root\n");
		return 1;
	}

	// Check argument for config file
	if (argc < 3) {
		fprintf(stderr, "Usage: %s %s <config file>\n", argv[0], argv[1]);
		return 1;
	}

	// Check access permissions on the config file
	if (access(argv[2], F_OK | R_OK) != 0) {
		fprintf(stderr, "error: could not access config file\n");
		return 1;
	}

	// Drop privileges until we after lua
	if (seteuid(getuid()) != 0) {
		fprintf(stderr, "error: could not drop privileges\n");
		return 1;
	}

	rb_lua_t ctx;
	ctx.config_file = (char *)argv[2];
	rb_lua_setup_context(&ctx);

	int status = luaL_dofile(ctx.L, ctx.config_file);
	if (status != LUA_OK) {
		fprintf(stderr, "Failed to execute lua configuration:\n");
		fprintf(stderr, "%s\n", lua_tostring(ctx.L, -1));
		lua_pop(ctx.L, 1);
		return 1;
	}

	// Restore privileges
	if (seteuid(0) != 0) {
		fprintf(stderr, "error: could not restore privileges\n");
		return 1;
	}

	rb_store_dump_revision(&ctx);
	lua_close(ctx.L);

	printf("Revision created successfully\n");
	return 0;
}
