--- @meta

--- @class rootbeer.scripts
--- Script writers — sugar over `rb.file()` for executable scripts.
---
--- Each helper writes the body to `path` with the appropriate shebang
--- prepended (`#!/usr/bin/env <interpreter>`) and marks the file
--- executable (mode 0755). Both ops defer until the apply stage.
---
--- These writers do **not** validate or lint the script body. The
--- interpreter is referenced by name and must be resolvable on the
--- target machine's `PATH` at run time — no packaging is implied.
---
--- ```lua
--- rb.scripts.bash("~/.local/bin/hello", [[
---   echo "hello $1"
--- ]])
--- ```

--- Generic script writer with a custom interpreter shebang. Use a bare
--- command (`"bash"`) which becomes `#!/usr/bin/env bash`, or an absolute
--- path (`"/bin/sh"`) which is used verbatim. Prefer the named helpers
--- (`bash`, `python`, …) for common languages.
--- @param interpreter string Bare command name or absolute interpreter path.
--- @param path string Destination file path (`~` is expanded).
--- @param body string Script body. A trailing newline is added if missing.
function rootbeer.scripts.script(interpreter, path, body) end

--- Writes an executable Bash script (`#!/usr/bin/env bash`).
--- Defers a `WriteFile` op for the body and a `Chmod` op (mode 0755).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.bash(path, body) end

--- Writes an executable POSIX `sh` script (`#!/usr/bin/env sh`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.sh(path, body) end

--- Writes an executable Zsh script (`#!/usr/bin/env zsh`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.zsh(path, body) end

--- Writes an executable Fish script (`#!/usr/bin/env fish`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.fish(path, body) end

--- Writes an executable Python 3 script (`#!/usr/bin/env python3`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.python(path, body) end

--- Writes an executable Node.js script (`#!/usr/bin/env node`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.node(path, body) end

--- Writes an executable Lua script (`#!/usr/bin/env lua`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.lua(path, body) end

--- Writes an executable Nushell script (`#!/usr/bin/env nu`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.nu(path, body) end

--- Writes an executable Ruby script (`#!/usr/bin/env ruby`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.ruby(path, body) end

--- Writes an executable Perl script (`#!/usr/bin/env perl`).
--- @param path string Destination file path.
--- @param body string Script body.
function rootbeer.scripts.perl(path, body) end
