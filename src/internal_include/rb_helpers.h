/**
 * @file rb_helpers.h
 * @brief Contains various helper functions for the `librootbeer` API.
 *
 * General purpose functions that can easily be extracted for reuse.
 * Good examples are @ref rb_canon_relative which canonicalizes an absolute
 * path to a relative path, etc.
 */
#ifndef RB_HELPERS_H
#define RB_HELPERS_H

#include "rb_ctx.h"

char *rb_canon_relative(rb_ctx_t *ctx, const char *abs_path);

#endif // RB_HELPERS_H
