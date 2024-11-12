#ifndef LUA_MODULE_H
#define LUA_MODULE_H

#include <lua.h>
#include <lualib.h>
#include <lauxlib.h>
#include <luajit.h>

// This file contains all of the functions that we can invoke from lua
// The src/config/functions directory contains implementations for each.
// All the functions are packed together into a single struct that is
// passed into lua context using luaL_newlib.

int rb_lua_debug_test(lua_State *L);

// These functions are all used to help load the module into lua

void rb_lua_create_module(lua_State *L);

#endif // LUA_MODULE_H
