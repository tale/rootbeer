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

// This file contains all of the functions that we can invoke from lua
// The src/config/functions directory contains implementations for each.
// All the functions are packed together into a single struct that is
// passed into lua context using luaL_newlib.

#define LUAFILES_MAX 100
#define REFFILES_MAX 100

// These functions are all used to help load the module into lua
// Additionally, we need a context struct to keep track of information
// that is only available from the lua side of things.

typedef struct {
	lua_State *L;
	char *config_root;
	char *config_file;

	char **req_filesv;
	int req_filesc;

	char **ref_filesv;
	int ref_filesc;

	char **gen_filesv; ///< Generated/output files in the revision
	int gen_filesc; ///< Count of generated files
} rb_lua_t;

void rb_lua_setup_context(rb_lua_t *ctx);
void rb_lua_register_module(lua_State *L);
rb_lua_t *rb_lua_get_ctx(lua_State *L);

int rb_lua_create_vm_sandbox(lua_State *L, const char *filename);

#endif // LUA_MODULE_H
