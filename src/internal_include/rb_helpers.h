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

#include <rb_ctx.h>

/**
 * Canonicalizes an absolute path to a relative path based on the context.
 * The path is relative to the entrypoint lua script directory.
 *
 * @param ctx Pointer to the rootbeer context.
 * @param abs_path The absolute path to be canonicalized.
 * @return A newly allocated string containing the relative path.
 */
char *rb_canon_relative(rb_ctx_t *ctx, const char *abs_path);

#endif // RB_HELPERS_H
