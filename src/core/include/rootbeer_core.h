#ifndef ROOTBEER_CORE_H
#define ROOTBEER_CORE_H

#include <lua.h>
#include <rb_plugin.h>
#include <rb_rootbeer.h>

char *rb_resolve_full_path(lua_State *L, const char *path);
int rb_core_ref_file(lua_State *L);
int rb_core_link_file(lua_State *L);
int rb_core_to_json(lua_State *L);
int rb_core_write_file(lua_State *L);
int rb_core_interpolate_table(lua_State *L);
int rb_core_register_module(lua_State *L);
int rb_core_emit(lua_State *L);
int rb_core_line(lua_State *L);
int rb_core_data(lua_State *L);
int rb_core_file(lua_State *L);

#endif
