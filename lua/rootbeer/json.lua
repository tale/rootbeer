--- @meta

--- @class rootbeer.json
--- JSON writer. Composable encode / decode / read / write functions sharing
--- one consistent shape across formats. Tables with consecutive integer keys
--- starting at 1 are treated as arrays; all other tables are objects.

--- Serializes a Lua table to a JSON string with 2-space indentation.
--- @param t table The table to serialize.
--- @return string The JSON-encoded string (no trailing newline).
function rootbeer.json.encode(t) end

--- Parses a JSON string into a Lua table.
--- @param s string The JSON-encoded string.
--- @return table The decoded value.
function rootbeer.json.decode(s) end

--- Reads and decodes a JSON file. Equivalent to `decode(slurp(path))`.
--- Path supports `~` expansion and is resolved against the script directory.
--- This call is **synchronous** — the file must exist at plan time.
--- @param path string The file to read.
--- @return table The decoded value.
function rootbeer.json.read(path) end

--- Encodes a table and writes it to a file. Equivalent to
--- `rb.file(path, encode(t))`. The write is deferred until the apply stage.
--- A trailing newline is always added so the file is well-formed.
--- @param path string The destination file path.
--- @param t table The table to serialize.
function rootbeer.json.write(path, t) end
