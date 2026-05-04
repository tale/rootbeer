# Script Writers

`rb.scripts` is sugar over `rb.file()` for executable scripts. Every
helper does the same three things:

1. Prepends `#!/usr/bin/env <interpreter>` (or the literal interpreter
   path if it begins with `/`).
2. Writes the body via the standard deferred `WriteFile` op.
3. Defers a `Chmod` op that sets mode `0755`.

Both ops participate in `rb plan` and `rb apply` like any other Rootbeer
side effect.

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

## What these writers do — and don't

Script writers do **not** validate the body, install the interpreter,
or manage `PATH`. The interpreter is referenced by name and must already
be resolvable on the target machine when the script runs — linting and
toolchain isolation are out of scope. A trailing newline is added if the
body doesn't already end in one, so the resulting file is always
well-formed.

## Named helpers

Each helper is `rb.scripts.<lang>(path, body)`:

| Helper                  | Shebang                      |
| ----------------------- | ---------------------------- |
| `rb.scripts.bash`       | `#!/usr/bin/env bash`        |
| `rb.scripts.sh`         | `#!/usr/bin/env sh`          |
| `rb.scripts.zsh`        | `#!/usr/bin/env zsh`         |
| `rb.scripts.fish`       | `#!/usr/bin/env fish`        |
| `rb.scripts.python`     | `#!/usr/bin/env python3`     |
| `rb.scripts.node`       | `#!/usr/bin/env node`        |
| `rb.scripts.lua`        | `#!/usr/bin/env lua`         |
| `rb.scripts.nu`         | `#!/usr/bin/env nu`          |
| `rb.scripts.ruby`       | `#!/usr/bin/env ruby`        |
| `rb.scripts.perl`       | `#!/usr/bin/env perl`        |

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
