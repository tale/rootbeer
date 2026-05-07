---
layout: home

hero:
  name: "ROOTBEER"
  tagline: Manage your system configuration with Lua.
  actions:
    - theme: brand
      text: What is Rootbeer?
      link: /guide/what-is-rootbeer
    - theme: alt
      text: Get Started
      link: /guide/getting-started
    - theme: alt
      text: Browse Modules
      link: /modules/

features:
  - icon: 🧑‍💻
    title: Config is Lua
    details: Write real Lua instead of templates or YAML. Use variables, functions, loops, and conditionals when they help.
    link: /guide/what-is-rootbeer#config-is-code
    linkText: Learn the model
  - icon: ⚡
    title: Plan Before Apply
    details: Rootbeer evaluates your config, builds a plan, and only writes changes when you run apply.
    link: /guide/what-is-rootbeer#plan-then-execute
    linkText: How it works
  - icon: 📦
    title: Modules for Real Tools
    details: Configure zsh, git, SSH, Homebrew, macOS, Amp, Claude Code, and more from typed Lua tables.
    link: /modules/
    linkText: Browse modules
  - icon: 🌐
    title: Profiles for Every Machine
    details: Keep one config repo and branch on roles like personal, work, server, or anything else you need.
    link: /guide/profiles
    linkText: Use profiles
---

<div class="home-code-preview">

## Quick look

```lua
local rb  = require("rootbeer")
local zsh = require("rootbeer.zsh")
local git = require("rootbeer.git")

rb.profile.define({
    strategy = "hostname",
    profiles = {
        personal = { "Aarnavs-MBP" },
        work     = { "tale-work" },
    },
})

zsh.config({
    env = { EDITOR = "nvim" },
    aliases = { g = "git", v = "nvim" },
    evals = { "mise activate zsh" },
})

git.config({
    user = {
        name = "Aarnav Tale",
        email = rb.profile.select({
            default = "aarnav@tale.me",
            work    = "aarnav@company.com",
        }),
    },
    signing = { key = "ssh-ed25519 AAAA..." },
    lfs = true,
})
```

</div>
