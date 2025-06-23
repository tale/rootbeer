/**
 * @file rb_ctx_state.h
 * @brief Runtime implementation details to manage the Rootbeer context.
 *
 * This file complements @ref rb_ctx.h by defining the functions only needed
 * by the runtime implementation of Rootbeer (the CLI). It has all of the lower
 * level details like dynamically allocating the context and converting it
 * to the data required to deterministically generate a revision.
 */
#ifndef RB_CTX_STATE_H
#define RB_CTX_STATE_H

#include "rb_ctx.h"

/**
 * Initializes the Rootbeer context.
 * This function initializes a blank Rootbeer context structure.
 * Because context is available as a light userdata in Lua, it needs to be
 * dynamically allocated along with its members.
 *
 * @return Pointer to the newly allocated Rootbeer context.
 */
rb_ctx_t *rb_ctx_init(void);

/**
 * Frees the Rootbeer context.
 * This function frees the Rootbeer context structure and all of its members.
 * It is used to clean up resources when the context is no longer needed.
 *
 * @param rb_ctx Pointer to the Rootbeer context to be freed.
 */
void rb_ctx_free(rb_ctx_t *rb_ctx);

#endif // RB_CTX_STATE_H
