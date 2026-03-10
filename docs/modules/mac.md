# mac

Configure macOS system preferences, Dock, Finder, hot corners, and more.

```lua
local mac = require("rootbeer.mac")

mac.dock({
    autohide = true,
    tile_size = 48,
    position = "bottom",
    show_recents = false,
    minimize_effect = "scale",
})

mac.finder({
    show_extensions = true,
    show_hidden = true,
    show_path_bar = true,
    default_view = "list",
    search_scope = "current",
})

mac.hot_corners({
    top_right = "notification_center",
    bottom_left = "lock_screen",
})

mac.input({
    tap_to_click = true,
    key_repeat_rate = 2,
    initial_key_repeat = 15,
})

mac.hostname({ name = "my-mac" })
mac.touch_id_sudo()
```

For one-off preferences not covered by the helpers above, use `mac.defaults()`
directly:

```lua
mac.defaults({
    { domain = "com.apple.LaunchServices", key = "LSQuarantine", type = "bool", value = false },
    { domain = "NSGlobalDomain", key = "NSAutomaticSpellingCorrectionEnabled", type = "bool", value = false },
})
```

<!--@include: ../api/_generated/mac.md-->
