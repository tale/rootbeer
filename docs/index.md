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
    details: Declare tools by name. Rootbeer installs verified binaries for your platform and records exact selections in your lockfile.
    link: /guide/packages
    linkText: Manage packages
  - title: Config Is Lua
    details: Use one language for packages, files, shell setup, and machine-specific configuration. Compose declarations with functions and modules.
    link: /guide/what-is-rootbeer
    linkText: Learn the model
  - title: Deliberate Updates
    details: Keep matching package locks stable. Refresh when you choose, review the changes, and replay cached installations offline.
    link: /guide/package-locks
    linkText: Control updates
  - title: One Config, Many Machines
    details: Share configuration across macOS and Linux, with profiles for personal, work, server, or any other role.
    link: /guide/profiles
    linkText: Use profiles
---

<div class="home-code-preview">

## One place for your environment

Start with [the package catalog](/packages/), copy the declarations for your tools,
and configure how you use them through [Lua modules](/modules/). Keep everything
in the same configuration repository, including `rootbeer.lock`.

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

zsh.config({
    sources = { rb.env_export("sh") },
    history = { size = 10000 },
})
```

```sh
rb apply --dry-run
rb apply
```

Rootbeer is a standalone binary. The package catalog grows independently of CLI
releases, with verified upstream binaries and packages built by the index workflow.

</div>
