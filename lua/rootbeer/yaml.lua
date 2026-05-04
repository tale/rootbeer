--- @meta

--- @class rootbeer.yaml
--- YAML writer. Composable encode / decode / read / write functions sharing
--- one consistent shape across formats. Tables with consecutive integer keys
--- starting at 1 are treated as sequences; all other tables are mappings.
--- YAML tags (`!!str`, custom tags, etc.) are decoded transparently to the
--- underlying value.

--- Serializes a Lua table to a YAML string.
--- @param t table The table to serialize.
--- @return string The YAML-encoded string (no trailing newline).
function rootbeer.yaml.encode(t) end

--- Parses a YAML string into a Lua table. Mapping keys that are not
--- already strings are stringified.
--- @param s string The YAML-encoded string.
--- @return table The decoded value.
function rootbeer.yaml.decode(s) end

--- Reads and decodes a YAML file. Equivalent to `decode(slurp(path))`.
--- Path supports `~` expansion and is resolved against the script directory.
--- This call is **synchronous** — the file must exist at plan time.
--- @param path string The file to read.
--- @return table The decoded value.
function rootbeer.yaml.read(path) end

--- Encodes a table and writes it to a file. Equivalent to
--- `rb.file(path, encode(t))`. The write is deferred until the apply stage.
--- A trailing newline is always added so the file is well-formed.
--- @param path string The destination file path.
--- @param t table The table to serialize.
function rootbeer.yaml.write(path, t) end
