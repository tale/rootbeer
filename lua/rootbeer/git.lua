--- @class git
local M = {}

local rb = require("rootbeer")
local tbl = require("rootbeer.tbl")

--- @class git.Config
--- @field path? string Where to write the gitconfig file. Defaults to `"~/.gitconfig"`.
--- @field user git.UserConfig User identity.
--- @field editor? string Default editor for commits (e.g. `"nvim"`).
--- @field pager? string Default pager for output (e.g. `"delta"`).
--- @field signing? git.SigningConfig Commit and tag signing. Sets `user.signingkey`, `gpg.format`, `commit.gpgSign`, and `tag.gpgSign`.
--- @field lfs? boolean Enable git-lfs filters (`filter.lfs` section).
--- @field pull_rebase? boolean Pull with rebase instead of merge.
--- @field merge_conflictstyle? string Merge conflict style (e.g. `"diff3"`).
--- @field ignores? string[] Global gitignore patterns. Written next to the gitconfig.
--- @field ignores_path? string Override path for the gitignore file. Defaults to `.gitignore` next to the gitconfig.
--- @field extra? table<string, table<string, string|boolean>> Additional gitconfig sections (e.g. `delta`, `interactive`). Merges into sections the other fields already wrote rather than replacing them, so `extra.core` keeps `excludesfile`. On a key conflict, `extra` wins.

--- @class git.UserConfig
--- @field name string Full name for commits.
--- @field email string Email address for commits.

--- @class git.SigningConfig
--- @field key string The signing key (e.g. an SSH public key).
--- @field format? string Signing format. Defaults to `"ssh"`.
--- @field allowed_signers? string[] Principals (usually emails) that `key` signs for. Writes an allowed signers file and points `gpg.ssh.allowedSignersFile` at it, which is what lets Git *verify* the signatures it creates. SSH format only.
--- @field allowed_signers_path? string Override path for the allowed signers file. Defaults to `"~/.ssh/allowed_signers"`.

--- Quotes a scalar for gitconfig format.
--- Strings are double-quoted; booleans and numbers are left bare.
--- Backslashes, quotes, and newlines are escaped so a value containing them
--- cannot terminate the string early and corrupt the surrounding section.
--- @param value string|number|boolean
--- @return string
local function quote(value)
	if type(value) ~= "string" then
		return tostring(value)
	end

	local escaped = value:gsub("\\", "\\\\"):gsub('"', '\\"'):gsub("\n", "\\n")
	return '"' .. escaped .. '"'
end

--- Recursively merges `src` into `dst`, with `src` winning on conflicts.
--- @param dst table
--- @param src table
local function merge_into(dst, src)
	for k, v in tbl.sorted_pairs(src) do
		if type(v) == "table" and type(dst[k]) == "table" then
			merge_into(dst[k], v)
		else
			dst[k] = v
		end
	end
end

--- Returns the directory portion of a path.
--- @param path string
--- @return string
local function dirname(path)
	return path:match("(.+)/") or "."
end

--- Emits a gitconfig-format string from a two-level table. Top-level keys
--- are sections; inner table values become `[section "subkey"]` blocks;
--- inner scalars become `\tkey = value` lines. Scalars are emitted
--- verbatim — callers should pre-quote strings via `quote()`.
--- @param sections table<string, table<string, string|table<string, string>>>
--- @return string
local function emit_gitconfig(sections)
	local parts = {}

	local function emit_block(header, body)
		if #parts > 0 then
			table.insert(parts, "")
		end
		table.insert(parts, header)
		for k, v in tbl.sorted_pairs(body) do
			if type(v) ~= "table" then
				table.insert(parts, "\t" .. k .. " = " .. tostring(v))
			end
		end
	end

	for section, values in tbl.sorted_pairs(sections) do
		-- subsections first: any inner key whose value is a table
		for k, v in tbl.sorted_pairs(values) do
			if type(v) == "table" then
				emit_block(string.format('[%s "%s"]', section, k), v)
			end
		end

		-- then the section's own scalars, if any
		local has_scalars = false
		for _, v in pairs(values) do
			if type(v) ~= "table" then
				has_scalars = true
				break
			end
		end
		if has_scalars then
			emit_block("[" .. section .. "]", values)
		end
	end

	return table.concat(parts, "\n") .. "\n"
end

--- Applies `git.Config` to the system. Writes a gitconfig file at `cfg.path`
--- and optionally a gitignore file next to it.
--- @param cfg git.Config
function M.config(cfg)
	local path = cfg.path or "~/.gitconfig"
	local ini = {}

	-- [user]
	ini.user = {
		name = cfg.user.name,
		email = cfg.user.email,
	}

	-- [core]
	local core = {}
	if cfg.editor then
		core.editor = cfg.editor
	end
	if cfg.pager then
		core.pager = cfg.pager
	end

	-- [commit], [tag], [gpg] via signing shortcut
	if cfg.signing then
		local format = cfg.signing.format or "ssh"
		ini.user.signingkey = cfg.signing.key
		ini.gpg = { format = format }
		ini.commit = { gpgSign = true }
		ini.tag = { gpgSign = true }

		-- Signing without this produces commits Git can create but not verify.
		if cfg.signing.allowed_signers and format == "ssh" then
			local signers_path = cfg.signing.allowed_signers_path
				or "~/.ssh/allowed_signers"
			local lines = {}
			for _, principal in ipairs(cfg.signing.allowed_signers) do
				lines[#lines + 1] = principal .. " " .. cfg.signing.key
			end

			rb.file(signers_path, table.concat(lines, "\n") .. "\n")
			ini.gpg.ssh = { allowedSignersFile = signers_path }
		end
	end

	-- ignores
	if cfg.ignores then
		local ignores_path = cfg.ignores_path
			or (dirname(path) .. "/.gitignore")
		core.excludesfile = ignores_path
		rb.file(ignores_path, table.concat(cfg.ignores, "\n") .. "\n")
	end

	if next(core) then
		ini.core = core
	end

	-- [pull]
	if cfg.pull_rebase ~= nil then
		ini.pull = { rebase = cfg.pull_rebase }
	end

	-- [merge]
	if cfg.merge_conflictstyle then
		ini.merge = { conflictstyle = cfg.merge_conflictstyle }
	end

	-- [filter "lfs"]
	if cfg.lfs then
		ini.filter = {
			lfs = {
				smudge = "git-lfs smudge -- %f",
				process = "git-lfs filter-process",
				required = true,
				clean = "git-lfs clean -- %f",
			},
		}
	end

	-- extra sections (delta, interactive, etc.). These merge into whatever the
	-- shortcuts above already wrote, so `extra.core` cannot drop `excludesfile`
	-- and `extra.tag` cannot drop `gpgSign`. On a key conflict, `extra` wins.
	if cfg.extra then
		for section, values in tbl.sorted_pairs(cfg.extra) do
			ini[section] = ini[section] or {}
			merge_into(ini[section], values)
		end
	end

	-- quote all string values for gitconfig format. Iteration order here
	-- doesn't matter since `emit_gitconfig` re-sorts; we just need to walk
	-- every entry to apply quoting.
	local quoted = {}
	for section, values in pairs(ini) do
		if type(values) == "table" then
			local q = {}
			for k, v in pairs(values) do
				if type(v) == "table" then
					local inner = {}
					for ik, iv in pairs(v) do
						inner[ik] = quote(iv)
					end
					q[k] = inner
				else
					q[k] = quote(v)
				end
			end
			quoted[section] = q
		end
	end

	rb.file(path, emit_gitconfig(quoted))
end

return M
