---
layout: home

hero:
  name: "ROOTBEER"
  tagline: Your packages and system configuration. Declared in Lua.
  actions:
    - theme: brand
      text: What is Rootbeer?
      link: /guide/what-is-rootbeer
    - theme: alt
      text: Get Started
      link: /guide/getting-started
    - theme: alt
      text: Find Packages
      link: /packages/

features:
  - title: Packages Are Configuration
    details: Install your tools from the same configuration as your shell and dotfiles.
    link: /guide/packages
    linkText: Manage packages
  - title: Config Is Lua
    details: Write your configuration in Lua. Use functions and modules to organize it as it grows.
    link: /guide/what-is-rootbeer
    linkText: Learn about Rootbeer
  - title: Update When You Choose
    details: Keep package versions the same between installs. Update them when you are ready.
    link: /guide/package-locks
    linkText: Control updates
  - title: One Config, Many Machines
    details: Share configuration across macOS and Linux, with profiles for personal, work, server, or any other role.
    link: /guide/profiles
    linkText: Use profiles
---

<div class="home-code-preview">

## One place for your environment

Install ripgrep and set up your shell in `init.lua`:

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

rb.package("ripgrep")

zsh.config({
    sources = { rb.env_export("sh") },
    history = { size = 10000 },
})
```

```sh
rb apply --dry-run
rb apply
```

Open a new terminal to use `rg`. [Get started](/guide/getting-started) to create
your own configuration, or [find more packages](/packages/).

</div>
