# Script Writers

Create executable scripts from your Lua configuration. Rootbeer adds the
interpreter line and makes the file executable when you run `rb apply`.

```lua
local rb = require("rootbeer")

rb.scripts.bash("~/.local/bin/hello", [[
  echo "hello $1"
]])

rb.scripts.python("~/.local/bin/greet", [[
  import sys
  print(f"hello, {sys.argv[1] if len(sys.argv) > 1 else 'world'}")
]])
```

The interpreter must be installed and available on `PATH` when the script runs.
Rootbeer does not check or lint the script body.

## Custom interpreters

Use `rb.scripts.script(interpreter, path, body)` for languages without a
named helper, or to pin a specific interpreter path:

```lua
rb.scripts.script("awk", "~/.local/bin/sum", [[
  { total += $1 } END { print total }
]])

rb.scripts.script("/opt/homebrew/bin/python3.12", "~/.local/bin/pinned", [[
  import sys; print(sys.version)
]])
```

A bare command (`"awk"`) becomes `#!/usr/bin/env awk`. An absolute path
(`/opt/homebrew/bin/python3.12`) is used verbatim as the shebang.

## API

<!--@include: ../api/_generated/scripts.md-->
