#include "rb_rootbeer.h"
#include "rootbeer_core.h"
#include "rb_ctx.h"
#include <fcntl.h>
#include <errno.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>
#include <libgen.h>
#include <sys/stat.h>

// From cli's fs.c — linked at build time
int rb_create_dir(char *path);

char *rb_resolve_full_path(lua_State *L, const char *path) {
	if (!path || path[0] == '\0') {
		luaL_error(L, "Invalid file path");
		return NULL;
	}

	// Handle ~ and ~/ prefix by expanding to $HOME
	if (path[0] == '~' && (path[1] == '/' || path[1] == '\0')) {
		const char *home = getenv("HOME");
		if (!home) {
			luaL_error(L, "$HOME is not set, cannot resolve '~'");
			return NULL;
		}

		size_t full_path_len = strlen(home) + strlen(path + 1) + 1;
		char *full_path = malloc(full_path_len);
		if (!full_path) {
			luaL_error(L, "Memory allocation failed for full path");
			return NULL;
		}

		snprintf(full_path, full_path_len, "%s%s", home, path + 1);
		return full_path;
	}

	if (path[0] == '/') {
		return strdup(path);
	}

	// Relative path, prepend CWD
	char cwd[PATH_MAX];
	if (getcwd(cwd, sizeof(cwd)) == NULL) {
		luaL_error(L, "Failed to get current working directory: %s", strerror(errno));
		return NULL;
	}

	size_t full_path_len = strlen(cwd) + 1 + strlen(path) + 1;
	char *full_path = malloc(full_path_len);
	if (!full_path) {
		luaL_error(L, "Memory allocation failed for full path");
		return NULL;
	}

	snprintf(full_path, full_path_len, "%s/%s", cwd, path);
	return full_path;
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

int rb_core_file(lua_State *L) {
	const char *raw_path = luaL_checkstring(L, 1);
	size_t len;
	const char *data = luaL_checklstring(L, 2, &len);

	char *filepath = rb_resolve_full_path(L, raw_path);
	if (!filepath) {
		return luaL_error(L, "Failed to resolve path '%s'", raw_path);
	}

	rb_ctx_t *ctx = rb_ctx_from_lua(L);

	if (ctx->dry_run) {
		printf("  write %s (%zu bytes)\n", filepath, len);
		free(filepath);
		return 0;
	}

	int status = rb_track_gen_file(ctx, filepath);
	if (status != RB_OK) {
		free(filepath);
		return luaL_error(L, "Failed to track file '%s': %d", filepath, status);
	}

	// Create parent directories
	char *dir_copy = strdup(filepath);
	char *parent = dirname(dir_copy);
	if (rb_create_dir(parent) != 0) {
		free(dir_copy);
		free(filepath);
		return luaL_error(L, "Failed to create directories for '%s'", raw_path);
	}
	free(dir_copy);

	int fd = open(filepath, O_WRONLY | O_CREAT | O_TRUNC, 0644);
	if (fd == -1) {
		free(filepath);
		return luaL_error(L, "Failed to open '%s': %s", raw_path, strerror(errno));
	}

	ssize_t written = write(fd, data, len);
	close(fd);

	if (written < 0 || (size_t)written != len) {
		unlink(filepath);
		free(filepath);
		return luaL_error(L, "Failed to write '%s': %s", raw_path, strerror(errno));
	}

	printf("  write %s\n", filepath);
	free(filepath);
	return 0;
}
