# Creating a Rootbeer Plugin
:::{tip}
This document covers creating a Rootbeer plugin in C. For information on
riting plugins in Lua, see the [Lua API documentation]({doc}`lua_api`).
:::

Plugins are what allow a user to define system configurations and behaviors.
Without plugins, Rootbeer is essentially a glorified Lua interpreter. Your
plugin is able to use the public Rootbeer API to hook into the revision
system and define extra files that can be used for a system configuration.

## Getting Started
At its core, a Rootbeer plugin is a static C library that is compiled into
the main `rb` executable. The plugin defines various functions that are
exposed into the Lua environment, allowing the user to interact with your
plugin.

To get started, you'll want to create a new directory in `src/plugins`
and create a `meson.build` file in that directory. This file will
look something like this:

```meson
rootbeer_lib = static_library(
	'myplugin_name',
	sources: files(
		'src/main.c',
		...
	),
	# Root include allows you to access the Rootbeer API headers
	include_directories: [include_directories('src'), root_include],

	# The only required dependency is luajit, which is used to
	# interact with the Lua environment. You can add more dependencies
	# as needed for the functionality of your plugin.
	dependencies: [dependency('luajit')],
)

# These 2 are VERY important, the name tells exactly how the plugin
# will be registered in the Lua environment (`rootbeer.myplugin_name`).
register_plugin_lib = rootbeer_lib
register_plugin_name = 'myplugin_name'
```

:::{warning}
In theory you can use another build system, but the Rootbeer build
system is designed to work with Meson. Try at your own risk.
:::

## Plugin Structure
Let's take a look at what is inside that `src/main.c` file. This is the
entry point for your plugin and is where you will define the functions
that will be exposed to the Lua environment. A simple plugin may look like this:

```c
#include "rb_plugin.h"

const luaL_Reg myplugin_funcs[] = {
	{"my_function", my_function},
	{NULL, NULL} // REQUIRED to terminate the array
};

// It's imperative that myplugin_name matches the name in the meson.build
// file in the `register_plugin_name` variable.
RB_PLUGIN(myplugin_name, "Description", "0.0.1", myplugin_funcs)
```

You can define your own functions in the `my_function` variable, which
will be called when the user calls `rootbeer.myplugin_name.my_function()`
from Lua. These plugins are expected to be global functions in your plugin.

From here you can get as complex as you want and split up your plugin
into multiple files. As long as you include the `rb_plugin.h` header
and the call to `RB_PLUGIN`, you can define as many functions as you want.

## `rb_plugin.h` Reference
```{doxygenfile} include/rb_plugin.h
:project: Rootbeer
