#include "rb_ctx.h"
#include "rb_rootbeer.h"
#include <stdlib.h>
#include <string.h>

#define RB_OUTPUT_INIT_CAP 4096

int rb_ctx_output_append(rb_ctx_t *ctx, const char *str, size_t len) {
	if (len == 0) {
		return RB_OK;
	}

	size_t needed = ctx->output_len + len + 1;
	if (needed > ctx->output_cap) {
		size_t new_cap = ctx->output_cap ? ctx->output_cap : RB_OUTPUT_INIT_CAP;
		while (new_cap < needed) {
			new_cap *= 2;
		}

		char *new_buf = realloc(ctx->output_buf, new_cap);
		if (new_buf == NULL) {
			return -1;
		}

		ctx->output_buf = new_buf;
		ctx->output_cap = new_cap;
	}

	memcpy(ctx->output_buf + ctx->output_len, str, len);
	ctx->output_len += len;
	ctx->output_buf[ctx->output_len] = '\0';
	return RB_OK;
}
