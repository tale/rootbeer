#include "rb_rootbeer.h"

int rb_track_gen_file(rb_lua_t *ctx, const char *path) {
	if (ctx->ref_filesc >= GENFILES_MAX) {
		return RB_ULIMIT_GENFILES;
	}

	if (access(path, F_OK | R_OK) != 0) {
		return RB_ENOENT;
	}

	// We need to copy the filename into the context
	ctx->gen_filesv[ctx->gen_filesc] = malloc(strlen(path) + 1);
	strcpy(ctx->gen_filesv[ctx->gen_filesc], path);
	ctx->gen_filesc++;

	return RB_OK;
}
