--- @meta

--- Primitives for managing files, creating symlinks, and querying system information.

--- Information about the current host machine and user.
--- See `host.lua` for the full type definition.
--- @type rootbeer.HostInfo
rootbeer.host = {}

--- Writes content to a file. Parent directories are created automatically.
--- Paths starting with `~` are expanded to `$HOME`; relative paths resolve
--- from the script directory.
--- @param path string The destination file path.
--- @param content string The content to write.
function rootbeer.file(path, content) end

--- Creates a symbolic link. The source path is relative to the script
--- directory and must exist. The destination supports `~` expansion.
--- Idempotent — existing correct links are skipped, stale links are replaced.
--- @param src string Source path relative to the script directory.
--- @param dst string Destination path (supports `~` expansion).
function rootbeer.link_file(src, dst) end

--- Executes a shell command. The command is deferred until the apply stage.
--- @param cmd string The command to run (e.g. `"brew"`).
--- @param args? string[] Optional arguments passed to the command.
function rootbeer.exec(cmd, args) end

--- Checks whether a path exists (file, directory, or symlink).
--- Supports `~` expansion and relative paths.
--- @param path string The path to check.
--- @return boolean
function rootbeer.path_exists(path) end

--- Checks whether a path is a regular file.
--- Supports `~` expansion and relative paths.
--- @param path string The path to check.
--- @return boolean
function rootbeer.is_file(path) end

--- Checks whether a path is a directory.
--- Supports `~` expansion and relative paths.
--- @param path string The path to check.
--- @return boolean
function rootbeer.is_dir(path) end

--- Returns a new list containing all elements from both input lists.
--- @param a any[] The first list.
--- @param b any[] The second list.
--- @return any[] The combined list.
function rootbeer.extend(a, b) end

--- @class rootbeer.Secret
rootbeer.secret = {}

--- Reads a secret from 1Password via the `op` CLI.
--- @param reference string The `op://` reference (e.g. `"op://vault/item/field"`).
--- @return string The secret value.
function rootbeer.secret.op(reference) end

--- @class rootbeer.Encode
rootbeer.encode = {}

--- Serializes a two-level table to an INI/gitconfig-formatted string.
--- Top-level keys must be tables and become `[section]` headers.
--- Scalar values within a section are emitted as `key = value`.
--- Nested table values become subsections (`[section "subsection"]`).
--- Nesting beyond two levels is ignored.
--- @param table table<string, table<string, string|number|boolean|table>> The table to serialize.
--- @return string The INI-encoded string.
function rootbeer.encode.ini(table) end

--- Serializes a Lua table to a JSON string with 2-space indentation.
--- Tables with consecutive integer keys starting at 1 are encoded as arrays.
--- All other tables are encoded as objects.
--- @param table table The table to serialize.
--- @return string The JSON-encoded string.
function rootbeer.encode.json(table) end

--- Serializes a Lua table to a TOML string.
--- Top-level scalar keys become key-value pairs. Nested tables become
--- `[section]` headers. Arrays of scalars use inline syntax; arrays of
--- tables use `[[array]]` syntax.
--- @param table table The table to serialize.
--- @return string The TOML-encoded string.
function rootbeer.encode.toml(table) end
