/**
 * @file lua_init.h
 * @brief Lua module initialization and context management.
 *
 * This file contains all the functions to manage the initialization of the
 * LuaJIT VM and the context for the Lua runtime. It includes the necessary
 * functions to set up the environment and support loading any rootbeer plugins.
 */
#ifndef LUA_INIT_H
#define LUA_INIT_H

#include "rb_ctx.h"
#include <lua.h>

/**
 * Bootstraps the LuaJIT VM and initializes the Lua context.
 * This function is called on `rb apply <file>` to set up the environment
 * relative to the file being run, pulling in any necessary lua libs.
 *
 * @param L Pointer to the Lua state.
 * @param entry_file The Lua file to be executed as the entry point.
 * @return 0 on success, or a non-zero error code on failure.
 */
int lua_runtime_init(lua_State *L, const char *entry_file);

/**
 * A faithful hook for the Lua require function.
 * When we setup the Lua environment, we replace the original `require` with
 * this hook which allows us to track which Lua files are required before
 * executing the original require function.
 *
 * @param L Pointer to the Lua state.
 * @return 1 on success, or a Lua error on failure.
 */
int lua_runtime_require_hook(lua_State *L);

/**
 * Registers the Lua context with the Lua state.
 * This function is used to register the context so that it can be accessed
 * from Lua scripts.
 *
 * @param L Pointer to the Lua state.
 * @param ctx Pointer to the rootbeer context.
 * @return 0 on success, or a non-zero error code on failure.
 */
int lua_register_context(lua_State *L, rb_ctx_t *ctx);

#endif // LUA_INIT_H
