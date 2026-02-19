#include "rb_rootbeer.h"
#include "rootbeer_core.h"
#include <errno.h>
#include <limits.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>
#include <libgen.h>
#include <sys/stat.h>
#include "rb_ctx.h"

int rb_create_dir(char *path);

int rb_core_link_file(lua_State *L) {
	const char *from = luaL_checkstring(L, 1);
	const char *to = luaL_checkstring(L, 2);

	rb_ctx_t *ctx = rb_ctx_from_lua(L);

	// 'from' is always relative to the script directory (source files)
	char from_buf[PATH_MAX];
	snprintf(from_buf, sizeof(from_buf), "%s/%s", ctx->script_dir, from);

	char resolved_from[PATH_MAX];
	if (realpath(from_buf, resolved_from) == NULL) {
		return luaL_error(L, "Cannot resolve source '%s': %s", from, strerror(errno));
	}

	if (access(resolved_from, F_OK | R_OK) != 0) {
		return luaL_error(L, "Cannot access source '%s': %s", resolved_from, strerror(errno));
	}

	// 'to' supports ~ expansion (target in home directory)
	char *resolved_to = rb_resolve_full_path(L, to);
	if (!resolved_to) {
		return luaL_error(L, "Failed to resolve target path '%s'", to);
	}

	// Check for dry-run mode
	if (ctx->dry_run) {
		printf("  link %s -> %s\n", resolved_from, resolved_to);
		free(resolved_to);
		return 0;
	}

	// Remove existing symlink or file at target
	struct stat st;
	if (lstat(resolved_to, &st) == 0) {
		if (S_ISLNK(st.st_mode)) {
			// Existing symlink — check if it already points to the right place
			char existing_target[PATH_MAX];
			ssize_t len = readlink(resolved_to, existing_target, sizeof(existing_target) - 1);
			if (len > 0) {
				existing_target[len] = '\0';
				if (strcmp(existing_target, resolved_from) == 0) {
					printf("  link %s (unchanged)\n", resolved_to);
					free(resolved_to);
					return 0;
				}
			}
			unlink(resolved_to);
		} else {
			free(resolved_to);
			return luaL_error(L, "Target '%s' exists and is not a symlink", to);
		}
	}

	// Create parent directories for target
	char *dir_copy = strdup(resolved_to);
	char *parent = dirname(dir_copy);
	if (rb_create_dir(parent) != 0) {
		free(dir_copy);
		free(resolved_to);
		return luaL_error(L, "Failed to create directories for '%s'", to);
	}
	free(dir_copy);

	if (symlink(resolved_from, resolved_to) != 0) {
		free(resolved_to);
		return luaL_error(L, "Failed to symlink '%s' -> '%s': %s", from, to, strerror(errno));
	}

	printf("  link %s -> %s\n", resolved_to, resolved_from);

	int status = rb_track_ref_file(ctx, from);
	if (status != 0) {
		free(resolved_to);
		return luaL_error(L, "Failed to track source file '%s': %d", from, status);
	}

	status = rb_track_gen_file(ctx, resolved_to);
	if (status != 0) {
		free(resolved_to);
		return luaL_error(L, "Failed to track target file '%s': %d", resolved_to, status);
	}

	free(resolved_to);
	return 0;
}
