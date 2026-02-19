--- @meta
--- Stubs for the native rootbeer core API (implemented in C).
--- This file is not loaded at runtime — it exists for LuaLS
--- type checking and lua2md documentation generation.

local rb = {}

--- @class rb.DataInfo
--- @field os string Operating system name (e.g. `"Linux"`, `"Darwin"`).
--- @field arch string CPU architecture (e.g. `"x86_64"`, `"arm64"`).
--- @field hostname string Machine hostname.
--- @field home string Home directory path.
--- @field username string Current username.

--- Writes content to a file path.
--- The `~` character is expanded to `$HOME`. Parent directories are
--- created automatically. In dry-run mode, prints what would be written.
--- @param path string The destination file path (supports `~` expansion).
--- @param content string The content to write.
function rb.file(path, content) end

--- Creates a symlink from a source file to a destination path.
--- The source is relative to the rootbeer source directory
--- (`~/.local/share/rootbeer/source/`). The destination supports `~`
--- expansion. Idempotent — existing correct links are left alone, stale
--- links are replaced.
--- @param src string Source path, relative to the source directory.
--- @param dest string Destination path (supports `~` expansion).
function rb.link_file(src, dest) end

--- Returns a table with information about the current machine.
--- @return rb.DataInfo
function rb.data() end

--- Serializes a Lua table to a JSON string.
--- @param tbl table The table to serialize.
--- @return string
function rb.to_json(tbl) end

--- Appends a line (with trailing newline) to the internal output buffer.
--- This is a low-level primitive — prefer `rb.file()` with module
--- renderers like `zsh.config()` for most use cases.
--- @param str string The line to append.
function rb.line(str) end

--- Appends raw text to the internal output buffer without a trailing newline.
--- @param str string The text to append.
function rb.emit(str) end

--- Passes a table through a transform function and returns the result.
--- @param tbl table The input table.
--- @param fn fun(tbl: table): table The transform function.
--- @return table
function rb.interpolate_table(tbl, fn) end

--- Registers a native Lua module under the given name.
--- Used internally by the plugin system.
--- @param name string The module name to register.
--- @param tbl table The module table.
function rb.register_module(name, tbl) end

return rb
