# Table helpers

`rootbeer.tbl` is a small set of table utilities for module authors. The
most important one is `sorted_pairs` — Lua's built-in `pairs()` has no
defined order, which produces nondeterministic generator output: different
content on every run, noisy diffs, unstable tests.

Module code that iterates a user-supplied map should prefer `sorted_pairs`
unless insertion order is explicitly part of the contract.

```lua
local tbl = require("rootbeer.tbl")

-- Deterministic iteration over a user map.
for alias, command in tbl.sorted_pairs(aliases) do
    print(alias, command)
end
```

See the [authoring principles](/contributing/architecture#authoring-principles)
for the full convention.

## API Reference

<!--@include: ../api/_generated/tbl.md-->
