# What is Rootbeer?

Rootbeer runs and installs command-line tools on macOS and Linux. It can also
manage your dotfiles and system settings from Lua.

## Start with a tool

Run a package without creating a configuration:

```sh
rb run ripgrep -- --hidden TODO .
```

Keep tools installed with `rb use jq ripgrep`. Rootbeer saves resolved versions
and verified downloads so you choose when to update. Browse the
[package catalog](/packages/) for commands, versions, and platform support.

## Config is code

When you want repeatable machine setup, declare packages and settings in Lua:

```lua
local rb = require("rootbeer")

rb.package("ripgrep")
rb.file("~/.inputrc", "set editing-mode vi\n")
```

[Modules](/modules/) provide configuration APIs for Zsh, Git, SSH, and other
tools. Use normal Lua functions, tables, and `require()` to organize your files.
[Profiles](/guide/profiles) select settings for different machines.

Keep the configuration in Git, including `rootbeer.lock`, to save the package
versions used by that configuration. You can adopt this workflow whenever you
need it; `rb run` and `rb use` work independently.

## Plan, then execute

`rb apply --dry-run` evaluates your Lua configuration and shows its planned
operations. `rb apply` executes them: installing packages, writing files, creating
symlinks, and running declared commands.

Rootbeer complements your system package manager. Its catalog supplies prebuilt
command-line tools; modules can configure software you installed elsewhere, and
[Homebrew integration](/modules/brew) can manage formulae and desktop applications.

[Get started](/guide/getting-started) with your first package, or go directly to
[writing a configuration](/guide/configuration).
