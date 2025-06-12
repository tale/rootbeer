#include "rb_rootbeer.h"
#include "rootbeer_core.h"
#include <errno.h>

// TODO: Make this a real header :skull:
int rb_create_dir(char *path);

int rb_core_link_file(lua_State *L) {
	const char *from = luaL_checkstring(L, 1);
	const char *to = luaL_checkstring(L, 2);

	if (from == NULL || to == NULL) {
		return luaL_error(L, "Invalid arguments: 'from' and 'to' must be non-null strings.");
	}

	rb_lua_t *ctx = rb_lua_get_ctx(L);

	// Need to resolve the realpath of the 'from' and 'to' paths.
	char filename_from[strlen(ctx->config_root) + strlen(from) + 2];
	snprintf(filename_from, sizeof(filename_from), "%s/%s", ctx->config_root, from);

	char filename_to[strlen(ctx->config_root) + strlen(to) + 2];
	snprintf(filename_to, sizeof(filename_to), "%s/%s", ctx->config_root, to);

	char resolved_from[PATH_MAX];
	if (realpath(filename_from, resolved_from) == NULL) {
		return luaL_error(L, "Failed to resolve 'from' path '%s': %s", filename_from, strerror(errno));
	}

	if (access(resolved_from, F_OK | R_OK) != 0) {
		return luaL_error(L, "Cannot access 'from' file '%s': %s", resolved_from, strerror(errno));
	}

	if (access(filename_to, F_OK) == 0) {
		// If the 'to' file already exists, we cannot link to it.
		return luaL_error(L, "Cannot link to '%s': file already exists.", filename_to);
	}


	// If we can resolve the files, symlink softlink them
	int status = rb_track_file(ctx, from);
	if (status != 0) {
		// TODO: rb_strerror needs to exist
		return luaL_error(L, "Failed to track file '%s': %d", from, status);
		// return luaL_error(L, "Failed to track file '%s': %s", from, rb_strerror(status));
	}


	char *parent_dir = dirname(filename_to);
	if (rb_create_dir(parent_dir) != 0) {
		return luaL_error(L, "Failed to create directory for '%s'.", parent_dir);
	}

	if (symlink(resolved_from, filename_to) != 0) {
		return luaL_error(L, "Failed to create symlink from '%s' to '%s': %s", from, to, strerror(errno));
	}

	// We don't need to track the symlink itself, as it will be resolved at runtime.
	// So now we are done.
	lua_pushboolean(L, 1);
	return 1;
}
