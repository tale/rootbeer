#include "rootbeer_core.h"
#include <fcntl.h>
#include <errno.h>

int rb_core_write_file(lua_State *L) {
	const char *filepath = luaL_checkstring(L, 1);
	size_t len;
	const char *data = luaL_checklstring(L, 2, &len);

	// Create parent directories if necessary (optional: not included here)
	// TODO: Move the fs.c from cli to librootbeer so it can be shared

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
