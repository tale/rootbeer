#include "rootbeer_core.h"
#include "rb_ctx.h"
#include <string.h>

int rb_core_emit(lua_State *L) {
	size_t len;
	const char *str = luaL_checklstring(L, 1, &len);

	rb_ctx_t *ctx = rb_ctx_from_lua(L);
	if (rb_ctx_output_append(ctx, str, len) != RB_OK) {
		return luaL_error(L, "Failed to append to output buffer");
	}

	return 0;
}

int rb_core_line(lua_State *L) {
	size_t len;
	const char *str = luaL_checklstring(L, 1, &len);

	rb_ctx_t *ctx = rb_ctx_from_lua(L);
	if (rb_ctx_output_append(ctx, str, len) != RB_OK) {
		return luaL_error(L, "Failed to append to output buffer");
	}

	if (rb_ctx_output_append(ctx, "\n", 1) != RB_OK) {
		return luaL_error(L, "Failed to append newline to output buffer");
	}

	return 0;
}
