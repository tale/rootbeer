---
layout: home

hero:
  name: "ROOTBEER"
  tagline: Define your system in Lua. One repo, every machine.
  actions:
    - theme: brand
      text: Get Started
      link: /guide/getting-started
    - theme: alt
      text: View on GitHub
      link: https://github.com/tale/rootbeer

features:
  - icon: 🧑‍💻
    title: Config is Code
    details: Lua — not templates, not YAML. Loops, conditionals, and functions out of the box.
    link: /guide/core-concepts
    linkText: Learn the model
  - icon: ⚡
    title: Plan & Apply
    details: Every call queues an operation. Preview the diff with -n, apply when ready. No surprises.
    link: /guide/core-concepts#plan-apply
    linkText: How it works
  - icon: 📦
    title: Declarative Modules
    details: zsh, git, SSH, Homebrew, macOS — describe the end state as a table, rootbeer writes the files.
    link: /modules/zsh
    linkText: Browse modules
  - icon: 🌐
    title: One Repo, Every Machine
    details: Profiles and host detection let you branch config per machine from a single dotfiles repo.
    link: /guide/multi-device
    linkText: Multi-device setup
---

<div class="home-code-preview">

## Quick look

```lua
local rb  = require("rootbeer")
local zsh = require("rootbeer.zsh")
local git = require("rootbeer.git")

zsh.config({
    env     = { EDITOR = "nvim" },
    aliases = { g = "git", v = "nvim" },
    evals   = { "mise activate zsh" },
})

git.config({
    user    = { name = "Aarnav Tale", email = "aarnav@tale.me" },
    signing = { key = "ssh-ed25519 AAAA..." },
    lfs     = true,
})
```

</div>
