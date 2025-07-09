#include "rb_helpers.h"
#include <assert.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>

char *rb_canon_relative(rb_ctx_t *ctx, const char *abs_path) {
	if (ctx == NULL || abs_path == NULL) {
		return NULL;
	}

	// Developer error misconfiguration
	assert(ctx->script_dir != NULL);
	char res_path[PATH_MAX];
	if (!realpath(abs_path, res_path)) {
		return NULL;
	}

	size_t dir_len = strlen(ctx->script_dir);
	if (strncmp(res_path, ctx->script_dir, dir_len) != 0 || res_path[dir_len] != '/') {
		// Not relative, we can just return the absolute path
		return strdup(res_path);
	}

	return strdup(res_path + dir_len + 1);
}
