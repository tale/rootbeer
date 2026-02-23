--- @meta

--- Primitives for managing files, creating symlinks, and querying system information.

--- @class rootbeer.SystemData
--- @field os string The operating system (e.g. `"macos"`, `"linux"`).
--- @field arch string CPU architecture (e.g. `"aarch64"`, `"x86_64"`).
--- @field home string Home directory path.
--- @field username string Current username.
--- @field hostname string Machine hostname.

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

--- Returns a table of information about the current system.
--- @return rootbeer.SystemData The system data table.
function rootbeer.data() end

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
