--- @class mac
local M = {}

local rb = require("rootbeer")

--- @class mac.DefaultEntry
--- @field domain string The defaults domain (e.g. `"com.apple.dock"`, `"NSGlobalDomain"`).
--- @field key string The preference key.
--- @field type "bool"|"int"|"float"|"string" The value type.
--- @field value string|number|boolean The value to set.

--- Writes macOS `defaults` entries. Each entry runs `defaults write domain key -type value`.
--- @param entries mac.DefaultEntry[]
function M.defaults(entries)
	for _, entry in ipairs(entries) do
		rb.exec("defaults", {
			"write",
			entry.domain,
			entry.key,
			"-" .. entry.type,
			tostring(entry.value),
		})
	end
end

--- @class mac.DockConfig
--- @field autohide? boolean Auto-hide the Dock. Defaults to `false`.
--- @field autohide_delay? number Delay before showing the Dock (seconds). Only applies when `autohide` is true.
--- @field magnification? boolean Enable icon magnification on hover.
--- @field tile_size? number Icon size in pixels (16–128).
--- @field large_size? number Magnified icon size in pixels. Only applies when `magnification` is true.
--- @field position? "bottom"|"left"|"right" Screen edge for the Dock.
--- @field minimize_effect? "genie"|"scale"|"suck" Window minimize animation.
--- @field show_recents? boolean Show recent applications section.
--- @field minimize_to_app? boolean Minimize windows into their app icon.

--- Configures the macOS Dock and restarts it to apply changes.
--- @param cfg mac.DockConfig
function M.dock(cfg)
	local entries = {}

	if cfg.autohide ~= nil then
		entries[#entries + 1] =
			{ key = "autohide", type = "bool", value = cfg.autohide }
	end
	if cfg.autohide_delay ~= nil then
		entries[#entries + 1] = {
			key = "autohide-delay",
			type = "float",
			value = cfg.autohide_delay,
		}
	end
	if cfg.magnification ~= nil then
		entries[#entries + 1] =
			{ key = "magnification", type = "bool", value = cfg.magnification }
	end
	if cfg.tile_size then
		entries[#entries + 1] =
			{ key = "tilesize", type = "int", value = cfg.tile_size }
	end
	if cfg.large_size then
		entries[#entries + 1] =
			{ key = "largesize", type = "int", value = cfg.large_size }
	end
	if cfg.position then
		entries[#entries + 1] =
			{ key = "orientation", type = "string", value = cfg.position }
	end
	if cfg.minimize_effect then
		entries[#entries + 1] =
			{ key = "mineffect", type = "string", value = cfg.minimize_effect }
	end
	if cfg.show_recents ~= nil then
		entries[#entries + 1] =
			{ key = "show-recents", type = "bool", value = cfg.show_recents }
	end
	if cfg.minimize_to_app ~= nil then
		entries[#entries + 1] = {
			key = "minimize-to-application",
			type = "bool",
			value = cfg.minimize_to_app,
		}
	end

	for _, e in ipairs(entries) do
		e.domain = "com.apple.dock"
	end

	M.defaults(entries)
	rb.exec("killall", { "Dock" })
end

--- @class mac.FinderConfig
--- @field show_extensions? boolean Always show file extensions.
--- @field show_hidden? boolean Show hidden files.
--- @field show_path_bar? boolean Show the path bar at the bottom.
--- @field show_status_bar? boolean Show the status bar at the bottom.
--- @field default_view? "list"|"icon"|"column"|"gallery" Default Finder view style.
--- @field search_scope? "current"|"mac"|"previous" Default search scope.

--- Configures macOS Finder and restarts it to apply changes.
--- @param cfg mac.FinderConfig
function M.finder(cfg)
	local entries = {}

	if cfg.show_extensions ~= nil then
		entries[#entries + 1] = {
			domain = "NSGlobalDomain",
			key = "AppleShowAllExtensions",
			type = "bool",
			value = cfg.show_extensions,
		}
	end

	if cfg.show_hidden ~= nil then
		entries[#entries + 1] = {
			domain = "com.apple.finder",
			key = "AppleShowAllFiles",
			type = "bool",
			value = cfg.show_hidden,
		}
	end

	if cfg.show_path_bar ~= nil then
		entries[#entries + 1] = {
			domain = "com.apple.finder",
			key = "ShowPathbar",
			type = "bool",
			value = cfg.show_path_bar,
		}
	end

	if cfg.show_status_bar ~= nil then
		entries[#entries + 1] = {
			domain = "com.apple.finder",
			key = "ShowStatusBar",
			type = "bool",
			value = cfg.show_status_bar,
		}
	end

	if cfg.default_view then
		local views = {
			icon = "icnv",
			list = "Nlsv",
			column = "clmv",
			gallery = "glyv",
		}
		entries[#entries + 1] = {
			domain = "com.apple.finder",
			key = "FXPreferredViewStyle",
			type = "string",
			value = views[cfg.default_view] or cfg.default_view,
		}
	end

	if cfg.search_scope then
		local scopes = {
			current = "SCcf",
			mac = "SCev",
			previous = "SCsp",
		}
		entries[#entries + 1] = {
			domain = "com.apple.finder",
			key = "FXDefaultSearchScope",
			type = "string",
			value = scopes[cfg.search_scope] or cfg.search_scope,
		}
	end

	M.defaults(entries)
	rb.exec("killall", { "Finder" })
end

--- @class mac.HotCorner
--- @field top_left? mac.HotCornerAction Action for top-left corner.
--- @field top_right? mac.HotCornerAction Action for top-right corner.
--- @field bottom_left? mac.HotCornerAction Action for bottom-left corner.
--- @field bottom_right? mac.HotCornerAction Action for bottom-right corner.

--- @alias mac.HotCornerAction
--- | "disabled"
--- | "mission_control"
--- | "app_windows"
--- | "desktop"
--- | "start_screensaver"
--- | "disable_screensaver"
--- | "notification_center"
--- | "launchpad"
--- | "quick_note"
--- | "lock_screen"
--- | "display_sleep"

--- Configures macOS hot corners.
--- @param cfg mac.HotCorner
function M.hot_corners(cfg)
	local action_ids = {
		disabled = 1,
		mission_control = 2,
		app_windows = 3,
		desktop = 4,
		start_screensaver = 5,
		disable_screensaver = 6,
		notification_center = 12,
		launchpad = 11,
		quick_note = 14,
		lock_screen = 13,
		display_sleep = 10,
	}

	local corners = {
		{ corner = "tl", key = cfg.top_left },
		{ corner = "tr", key = cfg.top_right },
		{ corner = "bl", key = cfg.bottom_left },
		{ corner = "br", key = cfg.bottom_right },
	}

	local entries = {}
	for _, c in ipairs(corners) do
		if c.key then
			local id = action_ids[c.key]
			if id then
				entries[#entries + 1] = {
					domain = "com.apple.dock",
					key = "wvous-" .. c.corner .. "-corner",
					type = "int",
					value = id,
				}
				entries[#entries + 1] = {
					domain = "com.apple.dock",
					key = "wvous-" .. c.corner .. "-modifier",
					type = "int",
					value = 0,
				}
			end
		end
	end

	M.defaults(entries)
end

--- @class mac.HostnameConfig
--- @field name string The hostname to set (used for ComputerName, HostName, and LocalHostName).

--- Sets the macOS hostname via `scutil`. Sets ComputerName, HostName,
--- and LocalHostName. Requires `sudo` to take effect.
--- @param cfg mac.HostnameConfig
function M.hostname(cfg)
	rb.exec("sudo", { "scutil", "--set", "ComputerName", cfg.name })
	rb.exec("sudo", { "scutil", "--set", "HostName", cfg.name })
	rb.exec("sudo", { "scutil", "--set", "LocalHostName", cfg.name })
end

--- Enables Touch ID for `sudo` by writing `/etc/pam.d/sudo_local`.
--- This is the Apple-recommended method that persists across macOS updates.
--- Requires `sudo` to take effect.
function M.touch_id_sudo()
	rb.exec("sudo", {
		"sh",
		"-c",
		"grep -q pam_tid /etc/pam.d/sudo_local 2>/dev/null || "
			.. 'echo "auth       sufficient     pam_tid.so" | sudo tee -a /etc/pam.d/sudo_local > /dev/null',
	})
end

--- @class mac.InputConfig
--- @field natural_scrolling? boolean Natural (content tracks finger) scrolling direction.
--- @field tap_to_click? boolean Enable tap-to-click on trackpad.
--- @field key_repeat_rate? number Key repeat rate (lower = faster, default is 6).
--- @field initial_key_repeat? number Delay before key repeat starts (lower = shorter, default is 25).

--- Configures macOS input preferences (keyboard and trackpad).
--- @param cfg mac.InputConfig
function M.input(cfg)
	local entries = {}

	if cfg.natural_scrolling ~= nil then
		entries[#entries + 1] = {
			domain = "NSGlobalDomain",
			key = "com.apple.swipescrolldirection",
			type = "bool",
			value = cfg.natural_scrolling,
		}
	end

	if cfg.tap_to_click ~= nil then
		entries[#entries + 1] = {
			domain = "com.apple.AppleMultitouchTrackpad",
			key = "Clicking",
			type = "bool",
			value = cfg.tap_to_click,
		}
		entries[#entries + 1] = {
			domain = "com.apple.driver.AppleBluetoothMultitouch.trackpad",
			key = "Clicking",
			type = "bool",
			value = cfg.tap_to_click,
		}
	end

	if cfg.key_repeat_rate then
		entries[#entries + 1] = {
			domain = "NSGlobalDomain",
			key = "KeyRepeat",
			type = "int",
			value = cfg.key_repeat_rate,
		}
	end

	if cfg.initial_key_repeat then
		entries[#entries + 1] = {
			domain = "NSGlobalDomain",
			key = "InitialKeyRepeat",
			type = "int",
			value = cfg.initial_key_repeat,
		}
	end

	M.defaults(entries)
end

return M
