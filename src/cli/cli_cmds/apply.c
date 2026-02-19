#include "cli_module.h"
#include "lua_init.h"
#include "rb_ctx.h"
#include "rb_ctx_state.h"
#include <lauxlib.h>
#include <libgen.h>
#include <limits.h>

void rb_cli_apply_print_usage() {
	printf("Usage: rootbeer apply [--dry-run|-n] [config file]\n");
	printf("\n");
	printf("If no config file is given, uses the default manifest at:\n");
	printf("  ~/.local/share/rootbeer/source/rootbeer.lua\n");
}

int rb_cli_apply_func(const int argc, const char *argv[]) {
	int dry_run = 0;
	const char *config_file = NULL;

	// Parse flags and find the config file argument
	for (int i = 2; i < argc; i++) {
		if (strcmp(argv[i], "--dry-run") == 0 || strcmp(argv[i], "-n") == 0) {
			dry_run = 1;
		} else if (argv[i][0] != '-') {
			config_file = argv[i];
		}
	}

	// If no config file given, use the default manifest
	char default_manifest[PATH_MAX];
	if (config_file == NULL) {
		const char *home = getenv("HOME");
		if (home == NULL) {
			fprintf(stderr, "error: $HOME is not set\n");
			return 1;
		}
		snprintf(default_manifest, sizeof(default_manifest),
			"%s%s/source/rootbeer.lua", home, RB_DATA_DIR_SUFFIX);
		config_file = default_manifest;
	}

	if (access(config_file, F_OK | R_OK) != 0) {
		fprintf(stderr, "error: could not access config file '%s'\n", config_file);
		return 1;
	}

	rb_ctx_t *rb_ctx = rb_ctx_init();
	rb_ctx->lua_state = luaL_newstate();
	char abs_config[PATH_MAX];
	if (realpath(config_file, abs_config) == NULL) {
		fprintf(stderr, "error: could not resolve path '%s'\n", config_file);
		rb_ctx_free(rb_ctx);
		return 1;
	}
	rb_ctx->script_path = strdup(abs_config);
	char *dir_tmp = strdup(abs_config);
	rb_ctx->script_dir = strdup(dirname(dir_tmp));
	free(dir_tmp);
	rb_ctx->dry_run = dry_run;

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

	if (dry_run) {
		printf("Dry run — no files will be written\n");
	}

	int status = luaL_dofile(rb_ctx->lua_state, rb_ctx->script_path);
	if (status != LUA_OK) {
		fprintf(stderr, "Failed to execute lua configuration:\n");
		fprintf(stderr, "%s\n", lua_tostring(rb_ctx->lua_state, -1));
		lua_pop(rb_ctx->lua_state, 1);
		rb_ctx_free(rb_ctx);
		return 1;
	}

	lua_close(rb_ctx->lua_state);
	rb_ctx_free(rb_ctx);

	if (dry_run) {
		printf("Dry run complete\n");
	} else {
		printf("Configuration applied successfully\n");
	}
	return 0;
}

rb_cli_cmd apply = {
	"apply",
	"Apply a lua configuration to your system",
	rb_cli_apply_print_usage,
	rb_cli_apply_func
};
