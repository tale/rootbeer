/**
 * @file lua_module.h
 * @brief Private shared-internals for librootbeer and the rootbeer CLI.
 *
 * This file mostly defines a bunch of the private lua internals that plugins
 * tend to be dependent on. While plugins should not be consuming these APIs,
 * librootbeer often bridges the gap between the plugins and the revision
 * awareness built into the rootbeer CLI.
 *
 * TODO: This file should be split into a shared-internals header file
 * and a lua module header file, so that rb_lua_t is the only public API.
 */
#ifndef LUA_MODULE_H
#define LUA_MODULE_H

#include <lua.h>
#include <lauxlib.h>
#include <luajit.h>
#include <lualib.h>
#include <unistd.h>
#include <stdlib.h>
#include <libgen.h>
#include <string.h>
#include <assert.h>

/**
 * @def LUA_LIB
 * Location of the rootbeer lua library.
 */
#define LUA_LIB "/opt/rootbeer/lua"

// This file contains all of the functions that we can invoke from lua
// The src/config/functions directory contains implementations for each.
// All the functions are packed together into a single struct that is
// passed into lua context using luaL_newlib.

/**
 * @def LUAFILES_MAX
 * Maximum number of lua files that can be loaded.
 */
#define LUAFILES_MAX 1000

/**
 * @def REFFILES_MAX
 * Maxmimum number of external files that can be tracked in a revision.
 */
#define REFFILES_MAX 1000

/**
 * @def GENFILES_MAX
 * Maximum number of generated files that can be tracked in a revision.
 */
#define GENFILES_MAX 1000

// These functions are all used to help load the module into lua
// Additionally, we need a context struct to keep track of information
// that is only available from the lua side of things.

/**
 * Represents the context for the lua module.
 * Everything here is in reference to the lua state and the file
 * that is run with `rb apply <file>`.
 */
typedef struct {
	lua_State *L; //!< Lua state for the module
	char *config_root; //!< Directory where the supplied file is located
	char *config_file; //!< The file that is being run with `rb apply <file>`

	char **req_filesv; //!< Files that are imported by lua scripts
	int req_filesc; //!< Count of lua files that are imported

	char **ref_filesv; //!< Reference files that are used in the revision
	int ref_filesc; //!< Count of reference files

	char **gen_filesv; //!< Generated files that are created by the revision
	int gen_filesc; //!< Count of generated files
} rb_lua_t;

/**
 * Initializes the lua context on init of the Lua VM.
 * This function is invoked on `rb apply <file>` and sets up the
 * lua context with the necessary information to run the
 * Lua scripts.
 *
 * This also is responsible for setting sandboxed permissions and loading
 * all of our main libraries along with the lua/ files that are relative
 * to the referenced file in the `rb apply <file>` command.
 *
 * @param ctx The lua context to initialize.
 */
void rb_lua_setup_context(rb_lua_t *ctx);

/**
 * Does all the heavy lifting of loading the actual Lua libraries.
 * In DEBUG mode, it will load the lua/ folder relative to the source-tree
 * rather than what is defined in the LUA_LIB macro.
 *
 * @param ctx The lua context to load the libraries into.
 */
void rb_lua_load_lib(rb_lua_t *ctx);

/**
 * Retrieves the lua context from the Lua state.
 * We store this context as a light userdata in the Lua state,
 * allowing anyone to technically access the context from Lua.
 *
 * While this may not be the best design, it is the most convenient
 * and provides better flexibility for plugins to access the context.
 * All API design is also done with the concept of passing the
 * rb_lua_t context around, so this is a convenient way
 * to access it from Lua scripts.
 *
 * @param L The Lua state to retrieve the context from.
 * @return The lua context associated with the Lua state.
 */
rb_lua_t *rb_lua_get_ctx(lua_State *L);

/**
 * Creates a Lua VM sandbox for the given file.
 * This takes an uninitialized Lua state and sets it up with the
 * necessary libraries and context to run the Lua scripts.
 * It also invokes LuaJIT with the supplied file, which is
 * resolved and executed in a sandboxed environment.
 *
 * @param L The Lua state to create the sandbox for.
 * @param filename The file to run in the Lua VM sandbox.
 * @return 0 on success, non-zero on failure.
 */
int rb_lua_create_vm_sandbox(lua_State *L, const char *filename);

#endif // LUA_MODULE_H
