# String helpers

`rootbeer.str` is a small set of string utilities for module authors. They
exist to keep generator code (Lua tables → tool config files) concise and
consistent across modules.

If you're writing your own integration that takes multi-line user input,
prefer these over hand-rolled `gmatch` loops — the built-in patterns have
a habit of silently dropping blank lines.

```lua
local str = require("rootbeer.str")

-- Preserves the blank line between blocks.
for _, line in ipairs(str.split_lines("step 1\n\nstep 2")) do
    print(line)
end

-- Indents every non-empty line, leaves blanks bare.
print(str.indent("a\n\nb", "\t"))
```

See the [authoring principles](/contributing/architecture#authoring-principles)
for guidance on when to use these versus inlining.

## API Reference

<!--@include: ../api/_generated/str.md-->
