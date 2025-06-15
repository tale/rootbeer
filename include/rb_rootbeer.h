/**
 * @file rb_rootbeer.h
 *
 * This header defines all of the publicly accessible APIs for Rootbeer plugins.
 * These can be called within a native Rootbeer plugin to interact with the
 * core Rootbeer revision system.
 * See \ghdir{src/plugins/rootbeer_core} for an example of a plugin.
 */
#ifndef RB_ROOTBEER_H
#define RB_ROOTBEER_H

#include "lua_module.h"

/**
 * Registers the provided filepath as a reference file.
 * Reference files are kept track of as "imported" files in the revision system.
 * For example, `rb.link_file()` uses this to track the source files for links.
 *
 * @param ctx The Lua context to track the file in.
 * @param path The ABSOLUTE path to the file to track.
 * @return 0 on success, or a negative error code on failure.
 * TODO: Make this use absolute paths instead of relative paths.
 */
int rb_track_ref_file(rb_lua_t *ctx, char *path);

/**
 * Registers the provided filepath as a generated file.
 * Generated files are those that are created by a plugin at runtime.
 * For example, `rb.link_file()` uses this to track destination files for links.
 *
 * @param ctx The Lua context to track the file in.
 * @param path The ABSOLUTE path to the file to track.
 * @return 0 on success, or a negative error code on failure.
 */
int rb_track_gen_file(rb_lua_t *ctx, const char *path);

/**
 * @def RB_OK
 * @brief The return code for a successful operation.
 */
#define RB_OK 0

/**
 * @def RB_ULIMIT_REFFILES
 * @brief The return code when the maximum reference files limit is reached.
 */
#define RB_ULIMIT_REFFILES -1001

/**
 * @def RB_ULIMIT_GENFILES
 * @brief The return code when the maximum generated files limit is reached.
 */
#define RB_ULIMIT_GENFILES -1002

/**
 * @def RB_ENOENT
 * @brief The return code when a file or directory does not exist.
 */
#define RB_ENOENT -2

/**
 * @def RB_EEXIST
 * @brief The return code when a file or directory already exists.
 */
#define RB_EACCES -13

#endif // RB_ROOTBEER_H
