--- @class git
local M = {}

local rb = require("@rootbeer")

--- @class git.Config
--- @field path string Where to write the gitconfig file.
--- @field user git.UserConfig User identity.
--- @field editor? string Default editor for commits (e.g. `"nvim"`).
--- @field pager? string Default pager for output (e.g. `"delta"`).
--- @field signing? git.SigningConfig Commit and tag signing. Sets `user.signingkey`, `gpg.format`, `commit.gpgSign`, and `tag.gpgSign`.
--- @field lfs? boolean Enable git-lfs filters (`filter.lfs` section).
--- @field pull_rebase? boolean Pull with rebase instead of merge.
--- @field merge_conflictstyle? string Merge conflict style (e.g. `"diff3"`).
--- @field ignores? string[] Global gitignore patterns. Written next to the gitconfig.
--- @field ignores_path? string Override path for the gitignore file. Defaults to `.gitignore` next to the gitconfig.
--- @field extra? table<string, table<string, string|boolean>> Additional gitconfig sections (e.g. `delta`, `interactive`).

--- @class git.UserConfig
--- @field name string Full name for commits.
--- @field email string Email address for commits.

--- @class git.SigningConfig
--- @field key string The signing key (e.g. an SSH public key).
--- @field format? string Signing format. Defaults to `"ssh"`.

--- Quotes a scalar for gitconfig format.
--- Strings are double-quoted; booleans and numbers are left bare.
--- @param value string|number|boolean
--- @return string
local function quote(value)
	if type(value) == "string" then
		return '"' .. value .. '"'
	end
	return tostring(value)
end

--- Returns the directory portion of a path.
--- @param path string
--- @return string
local function dirname(path)
	return path:match("(.+)/") or "."
end

--- Applies `git.Config` to the system. Writes a gitconfig file at `cfg.path`
--- and optionally a gitignore file next to it.
--- @param cfg git.Config
function M.config(cfg)
	local ini = {}

	-- [user]
	ini.user = {
		name = cfg.user.name,
		email = cfg.user.email,
	}

	-- [core]
	local core = {}
	if cfg.editor then core.editor = cfg.editor end
	if cfg.pager then core.pager = cfg.pager end

	-- [commit], [tag], [gpg] via signing shortcut
	if cfg.signing then
		ini.user.signingkey = cfg.signing.key
		ini.gpg = { format = cfg.signing.format or "ssh" }
		ini.commit = { gpgSign = true }
		ini.tag = { gpgSign = true }
	end

	-- ignores
	if cfg.ignores then
		local ignores_path = cfg.ignores_path or (dirname(cfg.path) .. "/.gitignore")
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

	-- extra sections (delta, interactive, etc.)
	if cfg.extra then
		for section, values in pairs(cfg.extra) do
			ini[section] = values
		end
	end

	-- quote all string values for gitconfig format
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

	rb.file(cfg.path, rb.encode.ini(quoted))
end

return M
