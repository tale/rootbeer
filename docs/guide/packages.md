# Packages

Install command-line tools from the same configuration as your shell and dotfiles.

## Install your tools

Add packages to `init.lua`. This example installs [ripgrep](https://github.com/BurntSushi/ripgrep):

```lua
local rb = require("rootbeer")

rb.package("ripgrep")
```

Apply, then make installed commands available in your current shell:

```sh
rb apply
eval "$(rb env)"
rg --version
```

[Find more packages](/packages/) to add to your configuration.

## Set up your shell

Add both lines, in this order, to `~/.bashrc` for Bash or `~/.zshrc` for Zsh:

```sh
export PATH="$HOME/.rootbeer/bin:$PATH"
eval "$(rb env)"
```

New terminals will then find your installed commands. Fish syntax is not supported.

### Configure Zsh

If you manage Zsh with Rootbeer, `zsh.config()` makes installed commands available
automatically in login shells. You do not need to add the lines above:

```lua
local zsh = require("rootbeer.zsh")

zsh.config({
    aliases = { g = "git" },
    history = { size = 10000 },
})
```

Zsh must already be installed. Run `rb apply`, then `zsh -l` to start a Zsh login
shell with your settings. This does not change your default shell.

## Choose a version

Without a version, Rootbeer installs the newest available release for your platform.
To choose an exact version, include it after `@`:

```lua
rb.package("ripgrep@15.2.0")
```

Rootbeer saves installed versions in `rootbeer.lock`. Commit this file with your
configuration. Running `rb apply` again keeps those versions unless your package
configuration or platform changes.

Run `rb apply --update` to update unpinned packages. Exact versions stay fixed.
See [updates and offline use](/guide/package-locks) for the other options.

## Supported packages

Rootbeer installs prebuilt command-line tools on macOS and Linux, on ARM64 and
x86-64. Use the platform filter in [package search](/packages/) to check a tool.
Some tools have older releases on platforms they no longer support; compatibility
with every Linux distribution or OS release is not guaranteed.

An unavailable package or exact version produces an error. Rootbeer does not
compile a replacement during installation.

Rootbeer does not yet install desktop applications, manage services, or
necessarily install every runtime dependency a tool needs. Continue using
[Homebrew](/modules/brew) or another package manager for those needs. Adding a
package to Rootbeer does not remove a copy installed by another manager.

For tools outside the catalog, see [other package sources](/guide/package-sources).
For sharing a configuration between platforms, see
[using multiple machines](/guide/package-locks#using-multiple-machines).
