---
layout: home

hero:
  name: "Rootbeer"
  text: Packages and system configuration.
  tagline: Run tools on macOS and Linux. Manage packages, dotfiles, and settings with Lua.
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: Browse packages ↗
      link: https://search.rbpkg.com
      target: _blank
      rel: noopener noreferrer

features:
  - title: Run and install tools
    details: Run a package once or install it for your user. No configuration required.
    link: /guide/packages
    linkText: Using packages
  - title: Configure your system
    details: Configure your shell, Git, SSH, and more with Lua. Preview changes before applying them.
    link: /guide/configuration
    linkText: Writing a configuration
  - title: Control updates
    details: Keep resolved package versions between installs. Update selected tools or your configured packages.
    link: /guide/package-locks
    linkText: Updates and offline use
  - title: Use multiple machines
    details: Share a configuration across macOS and Linux. Use profiles for settings that differ between machines.
    link: /guide/profiles
    linkText: Using profiles
---

<div class="home-code-preview">

## Run a package

```sh
rb run ripgrep -- --hidden TODO .
```

## Configure packages and settings

Declare packages and settings in `init.lua`:

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

rb.package("ripgrep")

zsh.config({
    history = { size = 10000 },
})
```

Preview and apply the configuration:

```sh
rb apply --dry-run
rb apply
eval "$(rb env)"
```

You can now use `rg`. Zsh must be installed to use the shell configuration;
start it with `zsh -l`. See [your configuration](/guide/configuration) for setup,
or [browse packages](https://search.rbpkg.com).

</div>
