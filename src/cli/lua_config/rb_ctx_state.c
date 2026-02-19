#include "rb_ctx_state.h"
#include "lua.h"
#include "rb_ctx.h"
#include "rb_strlist.h"
#include <stdlib.h>
#include <assert.h>

rb_ctx_t *rb_ctx_from_lua(lua_State *L) {
	lua_pushlightuserdata(L, (void *)rb_ctx_from_lua);
	lua_gettable(L, LUA_REGISTRYINDEX);
	rb_ctx_t *ctx = (rb_ctx_t *)lua_touserdata(L, -1);

	assert(ctx != NULL);
	return ctx;
}

rb_ctx_t *rb_ctx_init(void) {
	rb_ctx_t *ctx = malloc(sizeof(rb_ctx_t));
	if (ctx == NULL) {
		return NULL;
	}

	// We skip initializing the script_path and script_dir since
	// those will only be available once the CLI is invoked.
	ctx->script_path = NULL;
	ctx->script_dir = NULL;

	// TODO: Cleanup whatever is going on here & handle allocation failure
	rb_strlist_init(&ctx->lua_modules, RB_INIT_LUAMODULES_CAP);
	rb_strlist_init(&ctx->static_inputs, RB_INIT_STATICINPUTS_CAP);
	rb_idlist_init(&ctx->intermediates, RB_INIT_INTERMEDIATES_CAP);
	rb_strlist_init(&ctx->generated, RB_INIT_GENERATED_CAP);

	ctx->dry_run = 0;
	ctx->output_buf = NULL;
	ctx->output_len = 0;
	ctx->output_cap = 0;

	ctx->lua_files = malloc(RB_CTX_LUAFILES_MAX * sizeof(char *));
	if (ctx->lua_files == NULL) {
		free(ctx);
		return NULL;
	}

	ctx->lua_files_count = 0;
	for (size_t i = 0; i < RB_CTX_LUAFILES_MAX; i++) {
		ctx->lua_files[i] = NULL;
	}

	ctx->ext_files = malloc(RB_CTX_EXTFILES_MAX * sizeof(char *));
	if (ctx->ext_files == NULL) {
		free(ctx->lua_files);
		free(ctx);
		return NULL;
	}

	ctx->ext_files_count = 0;
	for (size_t i = 0; i < RB_CTX_EXTFILES_MAX; i++) {
		ctx->ext_files[i] = NULL;
	}

	ctx->plugin_transforms = malloc(RB_CTX_TRANSFORMS_MAX * sizeof(char *));
	if (ctx->plugin_transforms == NULL) {
		free(ctx->ext_files);
		free(ctx->lua_files);
		free(ctx);
		return NULL;
	}

	ctx->plugin_transforms_count = 0;
	for (size_t i = 0; i < RB_CTX_TRANSFORMS_MAX; i++) {
		ctx->plugin_transforms[i] = NULL;
	}

	return ctx;
}

void rb_ctx_free(rb_ctx_t *rb_ctx) {
	if (rb_ctx == NULL) {
		return;
	}

	for (size_t i = 0; i < rb_ctx->lua_files_count; i++) {
		free(rb_ctx->lua_files[i]);
	}


	for (size_t i = 0; i < rb_ctx->ext_files_count; i++) {
		free(rb_ctx->ext_files[i]);
	}

	for (size_t i = 0; i < rb_ctx->plugin_transforms_count; i++) {
		free(rb_ctx->plugin_transforms[i]);
	}

	free(rb_ctx->lua_files);
	free(rb_ctx->ext_files);
	free(rb_ctx->plugin_transforms);
	free(rb_ctx->output_buf);

	free(rb_ctx);
	return;
}
