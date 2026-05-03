--- @class amp
local M = {}

local rb = require("rootbeer")

--- @class amp.Config
--- @field path? string Where to write settings. Defaults to `"~/.config/amp/settings.json"`.
--- @field thinking? boolean Enable Claude's extended thinking (`amp.anthropic.thinking.enabled`).
--- @field mcp_servers? table<string, amp.McpServer> MCP server configurations (`amp.mcpServers`).
--- @field permissions? amp.Permission[] Permission rules (`amp.permissions`).
--- @field always_include_paths? string[] Glob patterns always included in fuzzy search (`amp.fuzzy.alwaysIncludePaths`).
--- @field updates_mode? "auto"|"warn"|"disabled" Update checking behavior (`amp.updates.mode`).

--- @class amp.McpServer
--- @field command? string Command to run (for local servers).
--- @field args? string[] Command arguments.
--- @field env? table<string, string> Environment variables.
--- @field url? string Server endpoint (for remote servers).
--- @field headers? table<string, string> HTTP headers.
--- @field include_tools? string[] Tool name filters.

--- @class amp.Permission
--- @field tool string Tool pattern to match.
--- @field mode "allow"|"deny"|"ask"|"delegate" Permission mode.

--- Writes the global `AGENTS.md` instructions file. This file is loaded
--- by Amp at the start of every session and applies across all projects.
--- Use it for personal coding conventions, preferred tools, and
--- project-agnostic guidance.
--- @param content string The markdown content for the instructions file.
--- @param path? string Where to write the file. Defaults to `"~/.config/amp/AGENTS.md"`.
function M.prompt(content, path)
	rb.file(path or "~/.config/amp/AGENTS.md", content)
end

--- Writes Amp settings to `cfg.path` as JSON.
--- @param cfg amp.Config
function M.config(cfg)
	local path = cfg.path or "~/.config/amp/settings.json"
	local settings = {}

	if cfg.thinking ~= nil then
		settings["amp.anthropic.thinking.enabled"] = cfg.thinking
	end

	if cfg.mcp_servers then
		settings["amp.mcpServers"] = cfg.mcp_servers
	end

	if cfg.permissions then
		settings["amp.permissions"] = cfg.permissions
	end

	if cfg.always_include_paths then
		settings["amp.fuzzy.alwaysIncludePaths"] = cfg.always_include_paths
	end

	if cfg.updates_mode then
		settings["amp.updates.mode"] = cfg.updates_mode
	end

	rb.json.write(path, settings)
end

return M
