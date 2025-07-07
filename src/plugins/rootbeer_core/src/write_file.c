#include "rb_rootbeer.h"
#include "rootbeer_core.h"
#include "rb_ctx.h"
#include <fcntl.h>
#include <errno.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>

char *rb_resolve_full_path(lua_State *L, const char *path) {
	// This function should resolve the full path based on the current working directory
	// and any other context-specific logic. For simplicity, we assume it returns the same path.
	// We can't use realpath here because the file does not exist yet.

	if (!path || path[0] == '\0') {
		luaL_error(L, "Invalid file path");
		return NULL;
	}

	if (path[0] == '/') {
		// Absolute path, return as is
		return strdup(path);
	} else {
		// Relative path, prepend current working directory
		char cwd[PATH_MAX];
		if (getcwd(cwd, sizeof(cwd)) == NULL) {
			luaL_error(L, "Failed to get current working directory: %s", strerror(errno));
			return NULL;
		}

		size_t full_path_len = strlen(cwd) + 1 + strlen(path) + 1; // cwd + '/' + path + '\0'
		char *full_path = malloc(full_path_len);
		if (!full_path) {
			luaL_error(L, "Memory allocation failed for full path");
			return NULL;
		}

		snprintf(full_path, full_path_len, "%s/%s", cwd, path);
		return full_path;
	}
}

int rb_core_write_file(lua_State *L) {
	const char *filepath = luaL_checkstring(L, 1);
	size_t len;
	const char *data = luaL_checklstring(L, 2, &len);

	// Create parent directories if necessary (optional: not included here)
	// TODO: Move the fs.c from cli to librootbeer so it can be shared

	rb_ctx_t *ctx = rb_ctx_from_lua(L);
	filepath = rb_resolve_full_path(L, filepath);
	if (!filepath) {
		return luaL_error(L, "Failed to resolve full path for '%s'", lua_tostring(L, 1));
	}

	int status = rb_track_gen_file(ctx, filepath);
	if (status != RB_OK) {
		// TODO: rb_strerror
		return luaL_error(L, "Failed to track file '%s': %d", filepath, status);
	}

	int fd = open(filepath, O_WRONLY | O_CREAT | O_TRUNC, 0644);
	if (fd == -1) {
		return luaL_error(L, "Failed to open file '%s': %s", filepath, strerror(errno));
	}

	ssize_t written = write(fd, data, len);
	close(fd);

	if (written < 0 || (size_t)written != len) {
		unlink(filepath);
		return luaL_error(L, "Failed to write to file '%s': %s", filepath, strerror(errno));
	}


	lua_pushstring(L, filepath);
	return 1;
}
