#include "rb_plugin.h"

#define EXTERN_PLUGIN(name) extern const rb_plugin_t rb_plugin_##name;
#define PLUGIN_ENTRY(name) &rb_plugin_##name,

RB_PLUGINS(EXTERN_PLUGIN)

const rb_plugin_t *rb_plugins[] = {
	RB_PLUGINS(PLUGIN_ENTRY)
	NULL
};
