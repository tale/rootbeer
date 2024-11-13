#include "rootbeer.h"
#include "lua_module.h"

int main(const int argc, const char *argv[]) {
	lua_State *L = luaL_newstate();
	int status = rb_lua_create_vm_sandbox(L, argv[1]);
	if (status != 0) {
		printf("error: could not create vm sandbox\n");
		lua_close(L);
		return 1;
	}

	lua_close(L);
	return 0;
}
