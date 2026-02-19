#include "rootbeer_core.h"
#include <cjson/cJSON.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>

static void table_to_json_object(lua_State *L, cJSON *json, int index);
static cJSON *lua_table_to_json(lua_State *L, int index) {
	if (index < 0) index = lua_gettop(L) + index + 1;
	cJSON *root = cJSON_CreateObject();
	table_to_json_object(L, root, index);
	return root;
}

int rb_core_to_json(lua_State *L) {
	luaL_checktype(L, 1, LUA_TTABLE);
	cJSON *json = lua_table_to_json(L, 1);
	if (!json) {
		lua_pushnil(L);
		return 1;
	}

	char *json_string = cJSON_PrintUnformatted(json);
	if (!json_string) {
		cJSON_Delete(json);
		lua_pushnil(L);
		return 1;
	}

	lua_pushstring(L, json_string);
	cJSON_Delete(json);
	free(json_string);
	return 1;
}

static void table_to_json_object(lua_State *L, cJSON *json, int index) {
	lua_pushnil(L);
	while (lua_next(L, index) != 0) {
		const char *key = lua_tostring(L, -2);
		if (!key) continue;

		switch (lua_type(L, -1)) {
			case LUA_TSTRING:
				cJSON_AddStringToObject(json, key, lua_tostring(L, -1));
				break;
			case LUA_TNUMBER:
				cJSON_AddNumberToObject(json, key, lua_tonumber(L, -1));
				break;
			case LUA_TBOOLEAN:
				cJSON_AddBoolToObject(json, key, lua_toboolean(L, -1));
				break;
			case LUA_TTABLE: {
				cJSON *sub = lua_table_to_json(L, lua_gettop(L));
				cJSON_AddItemToObject(json, key, sub);
				break;
			}
			default:
				// skip unsupported types
				printf("Skipping unsupported type for key '%s': %s\n", key, lua_typename(L, lua_type(L, -1)));
				break;
		}
		lua_pop(L, 1);
	}
}
