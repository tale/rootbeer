#include "rb_ctx.h"
#include "rb_rootbeer.h"
#include <errno.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int rb_track_gen_file(rb_ctx_t *ctx, const char *path) {
	if (ctx->plugin_transforms_count >= RB_CTX_TRANSFORMS_MAX) {
		return RB_ULIMIT_TRANSFORMS;
	}

	if (access(path, R_OK | W_OK) != 0) {
		// Check if the errno is EACCES, since we can ignore ENOENT
		if (errno == EACCES) {
			return RB_EACCES;
		}
	}

	ctx->plugin_transforms[ctx->plugin_transforms_count] = malloc(strlen(path) + 1);
	strncpy(ctx->plugin_transforms[ctx->plugin_transforms_count], path, strlen(path) + 1);
	ctx->plugin_transforms_count++;
	return RB_OK;
}
