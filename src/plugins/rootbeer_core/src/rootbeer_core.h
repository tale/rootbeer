#ifndef ROOTBEER_CORE_H
#define ROOTBEER_CORE_H

#include <lua.h>
#include <rb_plugin.h>
#include <rb_rootbeer.h>

int rb_core_ref_file(lua_State *L);
int rb_core_link_file(lua_State *L);
int rb_core_to_json(lua_State *L);
int rb_core_write_file(lua_State *L);
int rb_core_interpolate_table(lua_State *L);

#endif
