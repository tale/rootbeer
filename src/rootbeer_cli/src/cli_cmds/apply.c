#include "cli_module.h"
#include "lua_init.h"
#include "rb_ctx.h"
#include "rb_ctx_state.h"
#include <lauxlib.h>
#include <libgen.h>

// The apply command is where we tell rootbeer to interpret the lua
// configuration and create a new system configuration revision.

void rb_cli_apply_print_usage() {
	printf("Usage: rootbeer apply <config file>\n");
}

int rb_cli_apply_func(const int argc, const char *argv[]) {
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

	rb_ctx_t *rb_ctx = rb_ctx_init();
	rb_ctx->lua_state = luaL_newstate();
	rb_ctx->script_path = strdup((const char *)argv[2]);
	rb_ctx->script_dir = dirname(strdup((const char *)argv[2]));
	int i = lua_runtime_init(rb_ctx->lua_state, rb_ctx->script_path);
	if (i != 0) {
		fprintf(stderr, "error: could not initialize lua runtime\n");
		rb_ctx_free(rb_ctx);
		return 1;
	}

	int j = lua_register_context(rb_ctx->lua_state, rb_ctx);
	if (j != 0) {
		fprintf(stderr, "error: could not register context in lua\n");
		rb_ctx_free(rb_ctx);
		return 1;
	}

	int status = luaL_dofile(rb_ctx->lua_state, rb_ctx->script_path);
	if (status != LUA_OK) {
		fprintf(stderr, "Failed to execute lua configuration:\n");
		fprintf(stderr, "%s\n", lua_tostring(rb_ctx->lua_state, -1));
		lua_pop(rb_ctx->lua_state, 1);
		return 1;
	}

	// Restore privileges
	if (seteuid(0) != 0) {
		fprintf(stderr, "error: could not restore privileges\n");
		return 1;
	}

	// Print all the lua_files
	for (size_t i = 0; i < rb_ctx->lua_files_count; i++) {
		printf("Lua file: %s\n", rb_ctx->lua_files[i]);
	}

	lua_close(rb_ctx->lua_state);
	rb_ctx_free(rb_ctx);

	printf("Revision created successfully\n");
	return 0;
}

rb_cli_cmd apply = {
	"apply",
	"Apply a lua configuration and generate a new revision for your system",
	rb_cli_apply_print_usage,
	rb_cli_apply_func
};
