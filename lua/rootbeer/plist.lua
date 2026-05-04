--- @meta

--- @class rootbeer.plist
--- Apple property list (plist) writer. Encodes as **XML plist** — the
--- human-readable form used by macOS `defaults` and most files under
--- `~/Library/Preferences/`. Composable encode / decode / read / write
--- functions sharing one consistent shape across formats.
---
--- Lua tables with consecutive integer keys starting at 1 become arrays;
--- all other tables become dictionaries. `nil` values are omitted. Plist
--- `Date` and `Data` values decode to strings on read.

--- Serializes a Lua table to an XML plist string.
--- @param t table The table to serialize.
--- @return string The XML plist (no trailing newline).
function rootbeer.plist.encode(t) end

--- Parses a plist string (XML or binary) into a Lua table.
--- @param s string The plist contents.
--- @return table The decoded value.
function rootbeer.plist.decode(s) end

--- Reads and decodes a plist file. Equivalent to `decode(slurp(path))`.
--- Both XML and binary plists are accepted.
--- Path supports `~` expansion and is resolved against the script directory.
--- This call is **synchronous** — the file must exist at plan time.
--- @param path string The file to read.
--- @return table The decoded value.
function rootbeer.plist.read(path) end

--- Encodes a table and writes it to a file as an XML plist. Equivalent to
--- `rb.file(path, encode(t))`. The write is deferred until the apply stage.
--- A trailing newline is always added so the file is well-formed.
--- @param path string The destination file path.
--- @param t table The table to serialize.
function rootbeer.plist.write(path, t) end
