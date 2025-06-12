#ifndef RB_ROOTBEER_H
#define RB_ROOTBEER_H

#include "lua_module.h"

// Used to track a file in a revision that isn't a lua file.
// For example, if your plugin has a config file or uses an
// external file, you can track it here so that the
// revision system knows to keep track of it.
//
// The builtin link_file() function in the rootbeer module uses this
// to track any files that is symlinks into place for the user.
int rb_track_file(rb_lua_t *ctx, char *path);

#define RB_ULIMIT_REFFILES -1001
#define RB_ENOENT -2
#define RB_EACCES -13


#endif
