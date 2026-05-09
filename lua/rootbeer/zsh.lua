--- @class zsh
local M = {}

local rb = require("rootbeer")

--- @class zsh.Config
--- @field dir? string ZDOTDIR path. Defaults to `"~/.config/zsh"`. All zsh files are written here and a bootstrap `~/.zshenv` is created to set `ZDOTDIR`.
--- @field env? table<string, string> Environment variables written to `.zshenv` (available in all shells).
--- @field profile? zsh.ProfileConfig Login shell configuration written to `.zprofile`. Rootbeer's package profile environment is sourced automatically.
--- @field options? string[] `setopt` options (e.g. `"CORRECT"`, `"EXTENDED_GLOB"`).
--- @field keybind_mode? "emacs"|"vi" Input mode (`set -o emacs` or `set -o vi`).
--- @field variables? table<string, string> Shell variable assignments (not exported).
--- @field aliases? table<string, string> Shell aliases defined via `alias name="command"`.
--- @field prompt? string Raw `PS1` prompt string.
--- @field vcs_info? boolean|zsh.VcsInfoConfig Enable git branch info in the prompt via `vcs_info`. When `true`, uses default format `" (%b)"`. Automatically sets `PROMPT_SUBST`, adds a `precmd` hook, and autoloads `vcs_info`.
--- @field history? zsh.HistoryConfig History settings.
--- @field completions? zsh.CompletionConfig Completion system settings.
--- @field functions? table<string, string> Shell functions. Keys are names, values are the function body. Supports multi-line `[[...]]` strings.
--- @field widgets? string[] Function names to register as ZLE widgets via `zle -N`. The function must be defined in `functions`.
--- @field hooks? table<string, string|string[]> Zsh hook registrations via `add-zsh-hook`. Keys are hook names (e.g. `"precmd"`), values are function names or lists of function names.
--- @field keybindings? table<string, string> `bindkey` mappings (e.g. `{ ["^R"] = "my-widget" }`).
--- @field evals? string[] Commands wrapped in `eval "$(cmd)"`.
--- @field sources? string[] File paths to source.
--- @field extra? string|string[] Raw lines appended as-is to the zshrc.

--- @class zsh.ProfileConfig
--- @field evals? string[] Commands wrapped in `eval "$(cmd)"`.
--- @field sources? string[] File paths to source.
--- @field path_prepend? string[] Directories prepended to `$PATH`.
--- @field path_append? string[] Directories appended to `$PATH`.
--- @field extra? string|string[] Raw lines appended as-is.

--- @class zsh.HistoryConfig
--- @field file? string `HISTFILE` path. Defaults to `$ZDOTDIR/.zsh_history`.
--- @field size? number `HISTSIZE`. Defaults to `10000`.
--- @field save_size? number `SAVEHIST`. Defaults to `size`.
--- @field ignore? string `HISTIGNORE` pattern.
--- @field dedup? boolean Remove duplicate entries. Defaults to `true`.
--- @field share? boolean Share history across sessions. Defaults to `true`.
--- @field append? boolean Append to history file. Defaults to `true`.

--- @class zsh.VcsInfoConfig
--- @field formats? string Format string for `vcs_info`. Defaults to `" (%b)"`.
--- @field check_for_changes? boolean Enable dirty/staged indicators. Defaults to `true`.

--- @class zsh.CompletionConfig
--- @field enable? boolean Run `compinit`. Defaults to `true`.
--- @field menu_select? boolean Arrow-key menu selection. Defaults to `true`.
--- @field vi_nav? boolean Use hjkl for menu navigation. Defaults to `false`.
--- @field cache? string Cache directory for completion data.
--- @field styles? table<string, string> Raw `zstyle` entries (pattern → value).

--- Appends a line to the output buffer, with an optional blank line separator
--- when the buffer is non-empty and `sep` is true.
--- @param lines string[]
--- @param line string
--- @param sep? boolean
local function add(lines, line, sep)
	if sep and #lines > 0 then
		lines[#lines + 1] = ""
	end
	lines[#lines + 1] = line
end

--- Appends multiple lines to the output buffer with a leading separator.
--- @param lines string[]
--- @param new string[]
local function add_block(lines, new)
	if #new == 0 then
		return
	end
	if #lines > 0 then
		lines[#lines + 1] = ""
	end
	for _, l in ipairs(new) do
		lines[#lines + 1] = l
	end
end

--- Appends extra content (string or string[]) to the output buffer.
--- @param lines string[]
--- @param extra string|string[]|nil
local function add_extra(lines, extra)
	if not extra then
		return
	end
	if type(extra) == "string" then
		add(lines, extra, true)
	elseif type(extra) == "table" then
		add_block(lines, extra)
	end
end

--- Writes lines to a file, joining with newlines.
--- @param path string
--- @param lines string[]
local function write(path, lines)
	rb.file(path, table.concat(lines, "\n") .. "\n")
end

--- Builds the bootstrap ~/.zshenv that sets ZDOTDIR.
--- @param dir string
local function build_zshenv_bootstrap(dir)
	local lines = {}
	add(lines, "ZDOTDIR=" .. dir)
	add(lines, ". $ZDOTDIR/.zshenv")
	write("~/.zshenv", lines)
end

--- Builds <dir>/.zshenv from the env table.
--- @param dir string
--- @param env table<string, string>
local function build_zshenv(dir, env)
	local lines = {}
	for key, value in pairs(env) do
		lines[#lines + 1] = "export " .. key .. '="' .. value .. '"'
	end
	write(dir .. "/.zshenv", lines)
end

--- Builds <dir>/.zprofile from profile config.
--- @param dir string
--- @param profile zsh.ProfileConfig
local function build_zprofile(dir, profile)
	profile = profile or {}
	local lines = {}
	add_block(lines, { ". " .. rb.env_export("zsh") })

	if profile.evals then
		for _, cmd in ipairs(profile.evals) do
			lines[#lines + 1] = 'eval "$(' .. cmd .. ')"'
		end
	end

	if profile.sources then
		local block = {}
		for _, path in ipairs(profile.sources) do
			block[#block + 1] = ". " .. path
		end
		add_block(lines, block)
	end

	if profile.path_prepend then
		local block = {}
		for _, d in ipairs(profile.path_prepend) do
			block[#block + 1] = 'export PATH="' .. d .. ':$PATH"'
		end
		add_block(lines, block)
	end

	if profile.path_append then
		local block = {}
		for _, d in ipairs(profile.path_append) do
			block[#block + 1] = 'export PATH="$PATH:' .. d .. '"'
		end
		add_block(lines, block)
	end

	add_extra(lines, profile.extra)
	write(dir .. "/.zprofile", lines)
end

--- Builds <dir>/.zshrc from the main config fields.
--- @param dir string
--- @param cfg zsh.Config
local function build_zshrc(dir, cfg)
	local lines = {}

	if cfg.keybind_mode then
		add(lines, "set -o " .. cfg.keybind_mode)
	end

	if cfg.options then
		for _, opt in ipairs(cfg.options) do
			add(lines, "setopt " .. opt)
		end
	end

	if cfg.vcs_info then
		local vi = type(cfg.vcs_info) == "table" and cfg.vcs_info or {}
		local formats = vi.formats or " (%b)"
		local check = vi.check_for_changes ~= false

		local block = {}
		block[#block + 1] = "autoload -Uz vcs_info"
		if check then
			block[#block + 1] =
				'zstyle ":vcs_info:git:*" check-for-changes true'
		end
		block[#block + 1] = 'zstyle ":vcs_info:git:*" formats "'
			.. formats
			.. '"'
		block[#block + 1] = "autoload -Uz add-zsh-hook"
		block[#block + 1] = "add-zsh-hook precmd vcs_info"
		block[#block + 1] = "setopt PROMPT_SUBST"
		add_block(lines, block)
	end

	if cfg.variables then
		local block = {}
		for name, value in pairs(cfg.variables) do
			block[#block + 1] = name .. "=" .. value
		end
		add_block(lines, block)
	end

	if cfg.prompt then
		add(lines, "PS1='" .. cfg.prompt .. "'", true)
	end

	if cfg.aliases then
		local block = {}
		for name, command in pairs(cfg.aliases) do
			block[#block + 1] = "alias " .. name .. "='" .. command .. "'"
		end
		add_block(lines, block)
	end

	if cfg.history then
		local h = cfg.history
		local block = {}

		if h.append ~= false then
			block[#block + 1] = "setopt APPEND_HISTORY"
			block[#block + 1] = "setopt INC_APPEND_HISTORY"
			block[#block + 1] = "setopt EXTENDED_HISTORY"
		end
		if h.share ~= false then
			block[#block + 1] = "setopt SHARE_HISTORY"
		end
		if h.dedup ~= false then
			block[#block + 1] = "setopt HIST_IGNORE_DUPS"
			block[#block + 1] = "setopt HIST_IGNORE_ALL_DUPS"
			block[#block + 1] = "setopt HIST_FIND_NO_DUPS"
		end
		block[#block + 1] = "setopt HIST_IGNORE_SPACE"

		block[#block + 1] = ""
		local size = h.size or 10000
		block[#block + 1] = "HISTFILE=" .. (h.file or "$ZDOTDIR/.zsh_history")
		block[#block + 1] = "HISTSIZE=" .. size
		block[#block + 1] = "SAVEHIST=" .. (h.save_size or size)
		if h.ignore then
			block[#block + 1] = "HISTIGNORE='" .. h.ignore .. "'"
		end

		add_block(lines, block)
	end

	if cfg.completions then
		local c = cfg.completions
		local block = {}

		if c.vi_nav then
			block[#block + 1] = "zmodload zsh/complist"
			block[#block + 1] = "bindkey -M menuselect 'h' vi-backward-char"
			block[#block + 1] =
				"bindkey -M menuselect 'k' vi-up-line-or-history"
			block[#block + 1] =
				"bindkey -M menuselect 'j' vi-down-line-or-history"
			block[#block + 1] = "bindkey -M menuselect 'l' vi-forward-char"
			block[#block + 1] = ""
		end

		if c.enable ~= false then
			block[#block + 1] = "autoload -Uz compinit; compinit"
		end

		if c.menu_select ~= false then
			block[#block + 1] = "setopt MENU_COMPLETE"
			block[#block + 1] = "setopt AUTO_LIST"
			block[#block + 1] = "setopt COMPLETE_IN_WORD"
		end

		if c.cache then
			block[#block + 1] = "zstyle ':completion:*' use-cache on"
			block[#block + 1] = "zstyle ':completion:*' cache-path \""
				.. c.cache
				.. '"'
		end

		if c.styles then
			for pattern, value in pairs(c.styles) do
				block[#block + 1] = "zstyle '" .. pattern .. "' " .. value
			end
		end

		add_block(lines, block)
	end

	if cfg.functions then
		for name, body in pairs(cfg.functions) do
			local block = {}
			block[#block + 1] = name .. "() {"
			for fn_line in body:gmatch("[^\n]+") do
				block[#block + 1] = "\t" .. fn_line
			end
			block[#block + 1] = "}"
			add_block(lines, block)
		end
	end

	if cfg.widgets then
		local block = {}
		for _, name in ipairs(cfg.widgets) do
			block[#block + 1] = "zle -N " .. name
		end
		add_block(lines, block)
	end

	if cfg.hooks then
		local block = {}
		block[#block + 1] = "autoload -Uz add-zsh-hook"
		for hook, fns in pairs(cfg.hooks) do
			if type(fns) == "string" then
				block[#block + 1] = "add-zsh-hook " .. hook .. " " .. fns
			else
				for _, fn in ipairs(fns) do
					block[#block + 1] = "add-zsh-hook " .. hook .. " " .. fn
				end
			end
		end
		add_block(lines, block)
	end

	if cfg.keybindings then
		local block = {}
		for key, widget in pairs(cfg.keybindings) do
			block[#block + 1] = "bindkey '" .. key .. "' " .. widget
		end
		add_block(lines, block)
	end

	if cfg.evals then
		local block = {}
		for _, cmd in ipairs(cfg.evals) do
			block[#block + 1] = 'eval "$(' .. cmd .. ')"'
		end
		add_block(lines, block)
	end

	if cfg.sources then
		local block = {}
		for _, path in ipairs(cfg.sources) do
			block[#block + 1] = "source " .. path
		end
		add_block(lines, block)
	end

	add_extra(lines, cfg.extra)
	write(dir .. "/.zshrc", lines)
end

--- Applies the full zsh configuration. Generates `~/.zshenv` (bootstrap),
--- `<dir>/.zshenv`, `<dir>/.zprofile`, and `<dir>/.zshrc` from a single
--- config table.
--- @param cfg zsh.Config
function M.config(cfg)
	local dir = cfg.dir or "~/.config/zsh"
	build_zshenv_bootstrap(dir)

	if cfg.env then
		build_zshenv(dir, cfg.env)
	end

	build_zprofile(dir, cfg.profile)

	build_zshrc(dir, cfg)
end

return M
