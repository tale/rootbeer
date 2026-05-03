--- @meta

--- @class rootbeer.toml
--- TOML writer. Composable encode / decode / read / write functions sharing
--- one consistent shape across formats. Top-level scalar keys become
--- key-value pairs; nested tables become `[section]` headers. Arrays of
--- scalars use inline syntax; arrays of tables use `[[array]]` syntax.

--- Serializes a Lua table to a TOML string.
--- @param t table The table to serialize. Top-level value must be a table.
--- @return string The TOML-encoded string (no trailing newline).
function rootbeer.toml.encode(t) end

--- Parses a TOML string into a Lua table. TOML datetimes are returned as
--- strings (use a date library if you need richer types).
--- @param s string The TOML-encoded string.
--- @return table The decoded value.
function rootbeer.toml.decode(s) end

--- Reads and decodes a TOML file. Equivalent to `decode(slurp(path))`.
--- Path supports `~` expansion and is resolved against the script directory.
--- This call is **synchronous** — the file must exist at plan time.
--- @param path string The file to read.
--- @return table The decoded value.
function rootbeer.toml.read(path) end

--- Encodes a table and writes it to a file. Equivalent to
--- `rb.file(path, encode(t))`. The write is deferred until the apply stage.
--- A trailing newline is always added so the file is well-formed.
--- @param path string The destination file path.
--- @param t table The table to serialize.
function rootbeer.toml.write(path, t) end
