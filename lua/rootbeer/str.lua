--- @class rootbeer.str
--- String utilities for module authors. These exist to keep generator code
--- in the high-level Lua modules concise and consistent — see
--- `docs/contributing/architecture.md` for guidance on when to reach for
--- these helpers vs. inlining.
local M = {}

--- Splits a string on newlines, **preserving empty lines**.
---
--- Unlike `s:gmatch("[^\n]+")`, this keeps blank lines intact so multi-line
--- user input (function bodies, shell snippets, templates) round-trips
--- without losing readability. A single trailing newline is normalized
--- away to avoid introducing a phantom blank line at the end.
---
--- ```lua
--- local str = require("rootbeer.str")
--- for _, line in ipairs(str.split_lines("a\n\nb\n")) do
---   print(line) -- "a", "", "b"
--- end
--- ```
---
--- @param s string The string to split.
--- @return string[] lines One entry per line; empty lines become `""`.
function M.split_lines(s)
	local result = {}
	for line in (s .. "\n"):gmatch("([^\n]*)\n") do
		result[#result + 1] = line
	end
	if result[#result] == "" then
		result[#result] = nil
	end
	return result
end

--- Indents every non-empty line of `s` with `prefix`. Blank lines are left
--- bare, which keeps generated output diff-friendly and avoids trailing
--- whitespace warnings from linters.
---
--- ```lua
--- local str = require("rootbeer.str")
--- print(str.indent("a\n\nb", "\t"))
--- -- "\ta\n\n\tb"
--- ```
---
--- @param s string The string to indent.
--- @param prefix string The prefix to prepend to each non-empty line.
--- @return string indented The indented string.
function M.indent(s, prefix)
	local lines = M.split_lines(s)
	for i, line in ipairs(lines) do
		if line ~= "" then
			lines[i] = prefix .. line
		end
	end
	return table.concat(lines, "\n")
end

return M
