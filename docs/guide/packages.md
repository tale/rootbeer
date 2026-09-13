# Packages

Install command-line tools from the same configuration as your shell and dotfiles.

## Install your tools

Add packages to `init.lua`. This example installs [ripgrep](https://github.com/BurntSushi/ripgrep)
and makes its `rg` command available in Zsh:

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

rb.package("ripgrep")

zsh.config({
    sources = { rb.env_export("sh") },
})
```

If you already call `zsh.config()`, add the `sources` entry to that configuration.
Then apply:

```sh
rb apply
```

Open a new terminal and run `rg --version`. [Find more packages](/packages/)
to add to your configuration.

For Bash or another POSIX-compatible shell, source the file returned by
`rb.env_export("sh")` from your shell startup file. Fish syntax is not supported.

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
