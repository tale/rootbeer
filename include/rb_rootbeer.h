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

/**
 * @brief Rootbeer context structure.
 * This is an opaque pointer to the internal Rootbeer context.
 * See @ref rb_ctx_t for more details.
 */
#include "lua.h"
#include <stdio.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct rb_ctx_t rb_ctx_t;

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
int rb_track_ref_file(rb_ctx_t *ctx, const char *path);

/**
 * Registers the provided filepath as a generated file.
 * Generated files are those that are created by a plugin at runtime.
 * For example, `rb.link_file()` uses this to track destination files for links.
 *
 * @param ctx The Lua context to track the file in.
 * @param path The ABSOLUTE path to the file to track.
 * @return 0 on success, or a negative error code on failure.
 */
int rb_track_gen_file(rb_ctx_t *ctx, const char *path);

/**
 * Opens a handle to an "intermediate file". Intermediate files are used
 * to store temporary data that is not meant to be kept long-term.
 * They can be looked up by their ID at a later time.
 *
 * @param ctx The Lua context to open the intermediate file in.
 * @param id The ID of the intermediate file to open.
 * @return A file handle to the intermediate file, or NULL on failure.
 */
FILE *rb_open_intermediate(rb_ctx_t *ctx, const char *id);

/**
 * Retrieves the content of an intermediate file by its ID.
 * This function reads the content of the intermediate file
 * and returns it as a string.
 *
 * @param ctx The Lua context to retrieve the intermediate file from.
 * @param id The ID of the intermediate file to retrieve.
 * @return A pointer to the content of the intermediate file,
 */
const char *rb_get_intermediate(rb_ctx_t *ctx, const char *id);

/**
 * @def RB_OK
 * @brief The return code for a successful operation.
 */
#define RB_OK 0

/**
 * @def RB_ULIMIT_EXTFILES
 * @brief The return code when the maximum external files limit is reached.
 */
#define RB_ULIMIT_EXTFILES -1001

/**
 * @def RB_ULIMIT_TRANSFORMS
 * @brief The return code when the maximum plugin transforms limit is reached.
 */
#define RB_ULIMIT_TRANSFORMS -1002

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

/**
 * Fetches the current Rootbeer context from the Lua state.
 * We store the context in Lua via a light userdata pointer, whic allows us to
 * fetch it easily from the Lua state and use it flexibly across the project.
 * To identify the context, we use this fetch function as the ID for the data.
 *
 * @param L The Lua state from which to fetch the context.
 * @return A pointer to the Rootbeer context.
 * @note This function will panic if the context is not set in the Lua state.
 */
rb_ctx_t *rb_ctx_from_lua(lua_State *L);

/**
 * Appends a string to the context's output buffer, growing it as needed.
 * Used by rb.line() and rb.emit() primitives.
 *
 * @param ctx The Rootbeer context.
 * @param str The string to append.
 * @param len Length of the string to append.
 * @return 0 on success, -1 on allocation failure.
 */
int rb_ctx_output_append(rb_ctx_t *ctx, const char *str, size_t len);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // RB_ROOTBEER_H
