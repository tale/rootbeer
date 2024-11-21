#include "lua_module.h"

int rb_lua_ref_file(lua_State *L) {
	rb_lua_t *ctx = rb_lua_get_ctx(L);
	const char *str = luaL_checkstring(L, 1);

	if (ctx->ref_filesc == 100) { // TODO: Define these as constants
		return luaL_error(L, "Maximum file reference limit was reached");
	}

	// See if we can even resolve the file or not
	char filename[strlen(ctx->config_root) + strlen(str) + 1];
	sprintf(filename, "%s/%s", ctx->config_root, str);

	char resolved_path[PATH_MAX];
	if (realpath(filename, resolved_path) == NULL) {
		char error_buf[2048];
		sprintf(error_buf, "Could not resolve path %s", filename);
		return luaL_error(L, error_buf);
	}

	if (access(resolved_path, F_OK | R_OK) != 0) {
		char error_buf[2048];
		sprintf(error_buf, "Could not access file %s", filename);
		return luaL_error(L, error_buf);
	}

	// We need to copy the filename into the context
	ctx->ref_filesv[ctx->ref_filesc] = malloc(strlen(filename) + 1);
	strcpy(ctx->ref_filesv[ctx->ref_filesc], filename);
	ctx->ref_filesc++;

	return 0;
}
