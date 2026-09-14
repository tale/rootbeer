# Your configuration

Use a Lua configuration to keep packages, dotfiles, and settings together.
`rb run` and `rb use` remain available independently; you do not need a
configuration just to install tools.

## Create and open

```sh
rb init
rb edit
```

`rb init` creates `init.lua` in `~/.config/rootbeer/`, or
`$XDG_CONFIG_HOME/rootbeer/` if you set `XDG_CONFIG_HOME`. It also configures
completions and type checking for editors using Lua Language Server.

`rb edit` opens that directory in `$VISUAL` or `$EDITOR`. `rb cd` opens a shell
there. `rb apply` uses its `init.lua` regardless of your current directory.

## Declare changes

Start with packages and a file:

```lua
local rb = require("rootbeer")

rb.package("ripgrep")
rb.file("~/.inputrc", "set editing-mode vi\n")
```

`rb.file()` writes the supplied contents and creates parent directories. To keep
a file in your repository and link it into place instead, save your Readline
settings as `inputrc` beside `init.lua`, then replace the `rb.file()` call with:

```lua
rb.link_file("inputrc", "~/.inputrc")
```

The source is relative to the configuration directory; `~` in the destination
expands to your home directory. See the [core API](/reference/core) for file,
symlink, and command operations.

## Preview and apply

```sh
rb apply --dry-run
rb apply
```

Rootbeer first evaluates Lua into a plan, then executes the planned operations.
Previewing shows what the configuration asks to change before applying it.
Declared commands run during apply, so make repeated execution safe when using
`rb.exec()`.

Use `eval "$(rb env)"` to expose installed package commands in this shell, or
follow [shell setup](/guide/packages#set-up-your-shell) for new terminals.

Keep `init.lua`, supporting files, and `rootbeer.lock` in Git. The lock saves
resolved package versions; [updates and offline use](/guide/package-locks)
explains when it changes.

## Split into modules

Local `require()` paths start in your configuration directory. For example,
put this in `tools.lua`:

```lua
local rb = require("rootbeer")

return function()
    rb.package("jq")
    rb.package("ripgrep")
end
```

Then load it from `init.lua`:

```lua
local configure_tools = require("tools")

configure_tools()
```

Use dot-separated paths for nested files: `require("settings.shell")` loads
`settings/shell.lua`. Built-in modules use the same syntax, such as
`require("rootbeer.zsh")`.

Browse [integration modules](/modules/) for tool-specific settings. Use
[profiles](/guide/profiles) when packages or settings differ between machines,
and the [host API](/reference/host) to inspect the current machine.

## Reuse a configuration

Instead of creating a fresh configuration, initialize from an existing GitHub
repository, Git URL, or local directory:

```sh
rb init tale/dotfiles
rb init https://tangled.org/tale.me/dotfiles.git
rb init ~/code/dotfiles
```

Choose one source. Git repositories are cloned into the configuration directory;
a local directory is linked there. For a private GitHub repository with SSH keys
already set up, use `rb init --ssh tale/dotfiles`.

Review the configuration, then preview and apply it. On another platform, check
[lockfile behavior](/guide/package-locks#using-multiple-machines) before sharing
changes back to the same repository.
