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
int rb_track_ref_file(rb_lua_t *ctx, char *path);

/// Register the provided file path as a generated file.
/// The revision keeps track of these files so that it can
/// remove them when the revision is reverted.
/// @param ctx The Lua context to track the file in.
/// @param path The ABSOLUTE path to the file to track.
int rb_track_gen_file(rb_lua_t *ctx, const char *path);

#define RB_OK 0
#define RB_ULIMIT_REFFILES -1001
#define RB_ULIMIT_GENFILES -1002
#define RB_ENOENT -2
#define RB_EACCES -13


#endif
