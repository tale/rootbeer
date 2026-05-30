--- @class rootbeer.tbl
--- Table utilities for module authors. The most important one is
--- `sorted_pairs` — Lua's built-in `pairs()` has no defined order, which
--- produces nondeterministic generator output (different content on every
--- run, noisy diffs, unstable tests). Module code that iterates a
--- user-supplied map should prefer `sorted_pairs` unless insertion order
--- is explicitly part of the contract.
local M = {}

--- Returns the keys of a table as a sorted array. Useful when you need
--- the keys themselves (e.g. to count them or iterate twice). For the
--- common case of "iterate this map in order", prefer `sorted_pairs`.
---
--- ```lua
--- local tbl = require("rootbeer.tbl")
--- for _, k in ipairs(tbl.sorted_keys({ b = 1, a = 2 })) do
---   print(k) -- "a", "b"
--- end
--- ```
---
--- @generic K, V
--- @param t table<K, V> The table whose keys to extract.
--- @return K[] keys Keys of `t`, sorted ascending.
function M.sorted_keys(t)
	local keys = {}
	for k in pairs(t) do
		keys[#keys + 1] = k
	end
	table.sort(keys)
	return keys
end

--- A drop-in replacement for `pairs()` that yields `(key, value)` pairs in
--- sorted-key order. Use this anywhere generator output must be stable
--- across runs.
---
--- ```lua
--- local tbl = require("rootbeer.tbl")
--- for k, v in tbl.sorted_pairs({ b = 2, a = 1 }) do
---   print(k, v) -- "a 1", "b 2"
--- end
--- ```
---
--- @generic K, V
--- @param t table<K, V> The table to iterate.
--- @return fun(): K?, V? iterator
function M.sorted_pairs(t)
	local keys = M.sorted_keys(t)
	local i = 0
	return function()
		i = i + 1
		local k = keys[i]
		if k == nil then
			return nil
		end
		return k, t[k]
	end
end

return M
