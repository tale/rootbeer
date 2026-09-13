# Getting Started

## Install Rootbeer

```sh
sh -c "$(curl -fsSL https://rootbeer.tale.me/rb.sh)"
```

This installs `rb` to `~/.rootbeer/bin`. Follow the installer's instructions to
add it to your shell's `PATH`.

## Create your configuration

```sh
rb init
rb edit
```

Your configuration lives in `~/.config/rootbeer/`. Edit `init.lua` to choose your
tools and settings. For example, this installs ripgrep and sets up Zsh:

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

rb.package("ripgrep")

zsh.config({
    sources = { rb.env_export("sh") },
    aliases = { g = "git" },
    history = { size = 10000 },
})
```

[Find more packages](/packages/) or use [modules](/modules/) to configure Git,
SSH, and other tools. You can split your Lua config into files with `require()`
as it grows.

`rb init` also enables autocomplete and type checking in editors configured
with Lua Language Server.

## Apply your configuration

Preview the changes, then apply them:

```sh
rb apply --dry-run
rb apply
```

Open a new terminal to load your shell settings. Run `rg --version` to check
that ripgrep is available.

Keep your configuration in Git, including the generated `rootbeer.lock` file.
It saves package versions so subsequent installs use the same ones. See
[the package guide](/guide/packages) for version selection and other shells.

## Use an existing configuration

Pass a GitHub repository, Git URL, or local path to `rb init`:

```sh
rb init tale/dotfiles
rb init https://tangled.org/tale.me/dotfiles.git
rb init /path/to/local/repo
```

For a private GitHub repository, use `rb init --ssh tale/dotfiles` if your SSH
keys are already set up. You can also clone over HTTPS and run `rb remote ssh`
later, after setting up your keys.

Run `rb cd` to open a shell in your configuration directory, or `rb edit` to
open it in your editor.

## Update

```sh
rb update          # Update Rootbeer
rb apply --update  # Update your packages
```

## Next steps

- [Packages](/guide/packages): install tools and choose versions.
- [Modules](/modules/): configure your shell, Git, SSH, and more.
- [Profiles](/guide/profiles): use different settings on different machines.
- [API reference](/reference/): write files, create symlinks, and run commands.
