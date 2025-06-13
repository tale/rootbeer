#include "rb_rootbeer.h"
#include "lua_module.h"

int rb_track_ref_file(rb_lua_t *ctx, char *path) {
	if (ctx->ref_filesc >= REFFILES_MAX) {
		return RB_ULIMIT_REFFILES;
	}

	// See if we can even resolve the file or not
	char filename[strlen(ctx->config_root) + strlen(path) + 2];
	snprintf(filename, sizeof(filename), "%s/%s", ctx->config_root, path);

	char resolved_path[PATH_MAX];
	if (realpath(filename, resolved_path) == NULL) {
		return RB_ENOENT;
	}

	if (access(resolved_path, F_OK | R_OK) != 0) {
		return RB_EACCES;
	}

	// We need to copy the filename into the context
	ctx->ref_filesv[ctx->ref_filesc] = malloc(strlen(filename) + 1);
	strcpy(ctx->ref_filesv[ctx->ref_filesc], filename);
	ctx->ref_filesc++;

	return 0;
}
