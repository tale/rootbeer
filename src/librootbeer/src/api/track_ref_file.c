#include "rb_ctx.h"
#include "rb_rootbeer.h"
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <limits.h>

int rb_track_ref_file(rb_ctx_t *ctx, const char *path) {
	if (ctx->ext_files_count >= RB_CTX_EXTFILES_MAX) {
		return RB_ULIMIT_EXTFILES;
	}

	// See if we can even resolve the file or not
	char filename[strlen(ctx->script_dir) + strlen(path) + 2];
	snprintf(filename, sizeof(filename), "%s/%s", ctx->script_dir, path);

	char resolved_path[PATH_MAX];
	if (realpath(filename, resolved_path) == NULL) {
		return RB_ENOENT;
	}

	if (access(resolved_path, F_OK | R_OK) != 0) {
		return RB_EACCES;
	}

	ctx->ext_files[ctx->ext_files_count] = malloc(strlen(filename) + 1);
	strncpy(ctx->ext_files[ctx->ext_files_count], filename, strlen(filename) + 1);
	ctx->ext_files_count++;
	return RB_OK;
}
