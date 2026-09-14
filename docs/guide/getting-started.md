# Getting Started

## Install Rootbeer

The installer requires `curl` and `unzip`. On Ubuntu, install them with
`sudo apt install curl unzip`.

```sh
sh -c "$(curl -fsSL https://rootbeer.tale.me/rb.sh)"
export PATH="$HOME/.rootbeer/bin:$PATH"
```

This installs `rb` to `~/.rootbeer/bin` and makes it available in your current shell.

## Create your configuration

```sh
rb init
rb edit
```

Your configuration lives in `~/.config/rootbeer/`. Edit `init.lua` to choose your
tools and settings. This installs ripgrep:

```lua
local rb = require("rootbeer")

rb.package("ripgrep")
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

Make installed commands available in your current shell:

```sh
eval "$(rb env)"
rg --version
```

Add both lines, in this order, to `~/.bashrc` for Bash or `~/.zshrc` for Zsh:

```sh
export PATH="$HOME/.rootbeer/bin:$PATH"
eval "$(rb env)"
```

Future terminals will find Rootbeer and your installed commands. If you manage
Zsh with Rootbeer, follow the [Zsh setup](/guide/packages#configure-zsh).

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
