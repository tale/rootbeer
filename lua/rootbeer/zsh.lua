--- @class zsh
local M = {}

local rb = require("@rootbeer")

--- @class zsh.Config
--- @field path string Where to write the zshrc file.
--- @field options? string[] `setopt` options (e.g. `"CORRECT"`, `"EXTENDED_GLOB"`).
--- @field keybind_mode? "emacs"|"vi" Input mode (`set -o emacs` or `set -o vi`).
--- @field env? table<string, string> Environment variables exported via `export KEY="value"`.
--- @field path_prepend? string[] Directories prepended to `$PATH`.
--- @field path_append? string[] Directories appended to `$PATH`.
--- @field aliases? table<string, string> Shell aliases defined via `alias name="command"`.
--- @field prompt? string Raw `PS1` prompt string.
--- @field history? zsh.HistoryConfig History settings.
--- @field completions? zsh.CompletionConfig Completion system settings.
--- @field functions? table<string, string> Shell functions. Keys are names, values are the function body.
--- @field keybindings? table<string, string> `bindkey` mappings (e.g. `{ ["^R"] = "my-widget" }`).
--- @field evals? string[] Commands wrapped in `eval "$(cmd)"`.
--- @field sources? string[] File paths to source.
--- @field extra? string|string[] Raw lines appended as-is to the output.

--- @class zsh.HistoryConfig
--- @field file? string `HISTFILE` path. Defaults to `$ZDOTDIR/.zsh_history`.
--- @field size? number `HISTSIZE`. Defaults to `10000`.
--- @field save_size? number `SAVEHIST`. Defaults to `size`.
--- @field ignore? string `HISTIGNORE` pattern.
--- @field dedup? boolean Remove duplicate entries. Defaults to `true`.
--- @field share? boolean Share history across sessions. Defaults to `true`.
--- @field append? boolean Append to history file. Defaults to `true`.

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
	if #new == 0 then return end
	if #lines > 0 then
		lines[#lines + 1] = ""
	end
	for _, l in ipairs(new) do
		lines[#lines + 1] = l
	end
end

--- Applies `zsh.Config` to the system. Writes a zshrc file at `cfg.path`.
--- @param cfg zsh.Config
function M.config(cfg)
	local lines = {}

	-- keybind mode
	if cfg.keybind_mode then
		add(lines, "set -o " .. cfg.keybind_mode)
	end

	-- setopt
	if cfg.options then
		for _, opt in ipairs(cfg.options) do
			add(lines, "setopt " .. opt)
		end
	end

	-- env
	if cfg.env then
		local block = {}
		for key, value in pairs(cfg.env) do
			block[#block + 1] = 'export ' .. key .. '="' .. value .. '"'
		end
		add_block(lines, block)
	end

	-- PATH
	if cfg.path_prepend then
		local block = {}
		for _, dir in ipairs(cfg.path_prepend) do
			block[#block + 1] = 'export PATH="' .. dir .. ':$PATH"'
		end
		add_block(lines, block)
	end
	if cfg.path_append then
		local block = {}
		for _, dir in ipairs(cfg.path_append) do
			block[#block + 1] = 'export PATH="$PATH:' .. dir .. '"'
		end
		add_block(lines, block)
	end

	-- prompt
	if cfg.prompt then
		add(lines, "PS1='" .. cfg.prompt .. "'", true)
	end

	-- aliases
	if cfg.aliases then
		local block = {}
		for name, command in pairs(cfg.aliases) do
			block[#block + 1] = 'alias ' .. name .. '="' .. command .. '"'
		end
		add_block(lines, block)
	end

	-- history
	if cfg.history then
		local h = cfg.history
		local block = {}

		-- history options
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

		-- history variables
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

	-- completions
	if cfg.completions then
		local c = cfg.completions
		local block = {}

		if c.vi_nav then
			block[#block + 1] = "zmodload zsh/complist"
			block[#block + 1] = "bindkey -M menuselect 'h' vi-backward-char"
			block[#block + 1] = "bindkey -M menuselect 'k' vi-up-line-or-history"
			block[#block + 1] = "bindkey -M menuselect 'j' vi-down-line-or-history"
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
			block[#block + 1] = "zstyle ':completion:*' cache-path \"" .. c.cache .. "\""
		end

		if c.styles then
			for pattern, value in pairs(c.styles) do
				block[#block + 1] = "zstyle '" .. pattern .. "' " .. value
			end
		end

		add_block(lines, block)
	end

	-- functions
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

	-- keybindings
	if cfg.keybindings then
		local block = {}
		for key, widget in pairs(cfg.keybindings) do
			block[#block + 1] = "bindkey '" .. key .. "' " .. widget
		end
		add_block(lines, block)
	end

	-- evals
	if cfg.evals then
		local block = {}
		for _, cmd in ipairs(cfg.evals) do
			block[#block + 1] = 'eval "$(' .. cmd .. ')"'
		end
		add_block(lines, block)
	end

	-- sources
	if cfg.sources then
		local block = {}
		for _, path in ipairs(cfg.sources) do
			block[#block + 1] = "source " .. path
		end
		add_block(lines, block)
	end

	-- extra: string or table of strings, appended as-is
	if cfg.extra then
		if type(cfg.extra) == "string" then
			add(lines, cfg.extra, true)
		elseif type(cfg.extra) == "table" then
			add_block(lines, cfg.extra)
		end
	end

	rb.file(cfg.path, table.concat(lines, "\n") .. "\n")
end

return M
