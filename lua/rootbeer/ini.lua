--- @meta

--- @class rootbeer.ini
--- INI / gitconfig writer. **Write-only** — Rootbeer ships a minimal emitter
--- aimed at gitconfig and similar formats, but no parser. If you need to
--- round-trip INI, encode it yourself and use `rb.file()` directly.
---
--- The encoder supports two levels of nesting: top-level keys must be tables
--- and become `[section]` headers; nested tables become subsections of the
--- form `[section "subsection"]`. Scalar values within a section are emitted
--- as `key = value`. No string escaping is performed.

--- Serializes a two-level table to an INI / gitconfig-formatted string.
--- @param t table<string, table<string, string|number|boolean|table>> The table to serialize.
--- @return string The INI-encoded string (no trailing newline).
function rootbeer.ini.encode(t) end

--- Encodes a table and writes it to a file. Equivalent to
--- `rb.file(path, encode(t))`. The write is deferred until the apply stage.
--- A trailing newline is always added so the file is well-formed.
--- @param path string The destination file path.
--- @param t table<string, table<string, string|number|boolean|table>> The table to serialize.
function rootbeer.ini.write(path, t) end
