--- @class claude_code
local M = {}

local rb = require("rootbeer")

--- @class claude_code.Config
--- @field path? string Where to write settings. Defaults to `"~/.claude/settings.json"`.
--- @field permissions? claude_code.Permissions Tool permission rules.
--- @field env? table<string, string> Environment variables for Claude Code sessions.
--- @field hooks? table<string, any> Lifecycle hook configurations.
--- @field model? string Override default model (e.g. `"opus"`, `"sonnet"`).
--- @field language? string Claude's preferred response language.
--- @field cleanup_period_days? number Session cleanup period in days.
--- @field auto_updates_channel? "stable"|"latest" Release channel for auto-updates.
--- @field lsp? string[] LSP plugins to enable (e.g. `{"pyright-lsp", "typescript-lsp"}`). Automatically sets `ENABLE_LSP_TOOL=1` in env and adds entries to `enabledPlugins`.
--- @field enabled_plugins? table<string, boolean> Explicitly enable or disable plugins by `"name@marketplace"` key.

--- @class claude_code.Permissions
--- @field allow? string[] Tool patterns to auto-allow (e.g. `"Bash(npm run *)"`).
--- @field deny? string[] Tool patterns to deny (e.g. `"Read(.env)"`).
--- @field default_mode? "default"|"acceptEdits"|"askEdits"|"viewOnly"|"plan" Default permission mode.
--- @field additional_directories? string[] Extra directories Claude can access.

--- @class claude_code.PromptOpts
--- @field path? string Where to write the file. Defaults to `"~/.claude/CLAUDE.md"`.
--- @field lsp? boolean Append LSP code navigation guidance to the prompt. Tells Claude to prefer LSP over grep for definitions, references, and type info.

local LSP_NUDGE = [[

## Code Intelligence

Prefer LSP over Grep/Glob/Read for code navigation:
- `goToDefinition` / `goToImplementation` to jump to source
- `findReferences` to see all usages across the codebase
- `workspaceSymbol` to find where something is defined
- `documentSymbol` to list all symbols in a file
- `hover` for type info without reading the file
- `incomingCalls` / `outgoingCalls` for call hierarchy

Before renaming or changing a function signature, use
`findReferences` to find all call sites first.

Use Grep/Glob only for text/pattern searches (comments,
strings, config values) where LSP doesn't help.

After writing or editing code, check LSP diagnostics before
moving on. Fix any type errors or missing imports immediately.
]]

--- Writes the global `CLAUDE.md` instructions file. This file is loaded
--- by Claude Code at the start of every session and applies across all
--- projects. Use it for personal coding conventions, preferred tools,
--- and project-agnostic guidance. When `opts.lsp` is true, appends
--- guidance telling Claude to prefer LSP for code navigation.
--- @param content string The markdown content for the instructions file.
--- @param opts? claude_code.PromptOpts
function M.prompt(content, opts)
	opts = opts or {}
	local out = content
	if opts.lsp then
		out = out .. LSP_NUDGE
	end
	rb.file(opts.path or "~/.claude/CLAUDE.md", out)
end

--- Writes Claude Code settings to `cfg.path` as JSON.
--- @param cfg claude_code.Config
function M.config(cfg)
	local path = cfg.path or "~/.claude/settings.json"
	local settings = {}

	if cfg.permissions then
		settings.permissions = {
			allow = cfg.permissions.allow,
			deny = cfg.permissions.deny,
			defaultMode = cfg.permissions.default_mode,
			additionalDirectories = cfg.permissions.additional_directories,
		}
	end

	settings.env = cfg.env or {}
	if cfg.hooks then settings.hooks = cfg.hooks end
	if cfg.model then settings.model = cfg.model end
	if cfg.language then settings.language = cfg.language end
	if cfg.cleanup_period_days then settings.cleanupPeriodDays = cfg.cleanup_period_days end
	if cfg.auto_updates_channel then settings.autoUpdatesChannel = cfg.auto_updates_channel end

	-- LSP: enable the tool flag and register plugins
	if cfg.lsp then
		settings.env.ENABLE_LSP_TOOL = "1"
		settings.enabledPlugins = settings.enabledPlugins or {}
		for _, name in ipairs(cfg.lsp) do
			settings.enabledPlugins[name .. "@claude-plugins-official"] = true
		end
	end

	-- Merge explicit plugin overrides on top
	if cfg.enabled_plugins then
		settings.enabledPlugins = settings.enabledPlugins or {}
		for k, v in pairs(cfg.enabled_plugins) do
			settings.enabledPlugins[k] = v
		end
	end

	-- Drop empty env table to keep output clean
	if next(settings.env) == nil then settings.env = nil end

	rb.json.write(path, settings)
end

return M
