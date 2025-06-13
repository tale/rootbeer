#include "rb_rootbeer.h"
#include <errno.h>

int rb_track_gen_file(rb_lua_t *ctx, const char *path) {
	if (ctx->ref_filesc >= GENFILES_MAX) {
		return RB_ULIMIT_GENFILES;
	}

	if (access(path, R_OK | W_OK) != 0) {
		// Check if the errno is EACCES, since we can ignore ENOENT
		if (errno == EACCES) {
			return RB_EACCES;
		}
	}

	// We need to copy the filename into the context
	ctx->gen_filesv[ctx->gen_filesc] = malloc(strlen(path) + 1);
	strcpy(ctx->gen_filesv[ctx->gen_filesc], path);
	ctx->gen_filesc++;

	return RB_OK;
}
