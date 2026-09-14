# Packages

[Browse packages](/packages/) to find a tool, check its commands and supported
platforms, and choose a version. You can run it immediately, keep it installed,
or declare it in your dotfile configuration.

## Run a tool

No configuration or shell setup is required:

```sh
rb run jq -- --version
rb run ripgrep -- --hidden TODO .
```

Arguments after `--` go to the tool. Its commands are available on PATH for that
process and its children; your current shell stays unchanged. Downloads are cached
for later runs.

### Package names and commands

A package name identifies what to install. Its commands are the executables you
run: the `ripgrep` package provides `rg`. Package aliases are alternate names for
requesting the same package; `rg` is also a declared alias of `ripgrep`.

```sh
rb run rg -- --version
rb run xz --bin xzdec -- --version
```

Rootbeer chooses a command matching your request or the canonical package name,
then the only command if the package exports just one. Use `--bin` to select
another exported command. The package browser lists commands and aliases separately.

Add `-p` for each additional package the command needs. For example, benchmark
`jq` with `hyperfine`:

```sh
rb run hyperfine -p jq -- 'jq --version'
```

### macOS app bundles

Open an app preserved inside a package by naming its bundle:

```sh
rb run bobrwm --app Bobrwm.app
rb run bobrwm --app Bobrwm.app -- --config "$HOME/.config/bobrwm/config.zon"
```

`--app` opens that exact bundle from the verified package store without creating
an Applications link. Arguments after `--` go to the app;
macOS may reuse an already running instance. Use `--bin` separately for command-line
tools. App permissions, such as Bobrwm's Accessibility access, remain managed by
macOS.

To keep a declared app export available in `~/Applications`, install its package:

```sh
rb use bobrwm
```

If you installed Bobrwm before app exports were added, run `rb update`, then
`rb use bobrwm --update` to refresh its package metadata.

Or declare `rb.package("bobrwm")` in Lua and run `rb apply`. Both manage a
`~/Applications/Bobrwm.app` symlink into the verified package store. Existing apps
are never overwritten; a conflicting app or unmanaged link makes installation fail.
User and Lua profiles may share an app link when they select the exact same
stored bundle; different versions conflict. Rootbeer does not configure login
items or grant app permissions.

Remove a package from your user profile with `rb unuse bobrwm`. This removes its
owned app link when no other profile needs it, while retaining cached package files.
For Lua-managed apps, remove the declaration and run `rb apply`.

## Keep tools installed

```sh
rb use jq ripgrep
eval "$(rb env)"
rg --version
```

`rb use` adds packages to your user profile. Requesting another version replaces
that package while keeping your other tools. The commands stay available in later
terminals once you add [shell setup](#set-up-your-shell).

This profile is independent of `init.lua` and `rb apply`. Its commands take
precedence when both profiles provide the same name.

## Declare tools in your configuration

Use Lua when you want to keep your package list alongside your shell and dotfiles:

```lua
local rb = require("rootbeer")

rb.packages({ "jq", "ripgrep", "gh" })
```

```sh
rb apply
eval "$(rb env)"
rg --version
```

Commit `init.lua` and the resulting `rootbeer.lock` to keep the configuration and
resolved package versions together.

Entries can be request strings, request tables with GitHub options, or exact
package specifications. The list must be dense: no missing indexes or named keys.
Rootbeer validates the whole list before adding operations and identifies a bad
entry by its index. `rb.package(...)` accepts one entry when you only need one.

Keep platform and profile selection in normal Lua, using `rb.host` or
[`rb.profile.when`](/guide/profiles). See [other sources](/guide/package-sources)
for request tables and the [API reference](/reference/core#rootbeer-packages)
for exact specifications.

### Run a declared command during apply

Use its stable configuration-profile path even on the first installation:

```lua
rb.packages({ "gh" })
rb.exec(rb.bin_path("gh"), { "--version" })
```

`rb.bin_path()` constructs the path without checking whether it is installed yet;
`rb.exec()` runs during apply, after the earlier package declaration.
`rb.bin_dir()` returns the configuration profile's binary directory.
`rb.which()` is an optional query for a known or installed managed command and
returns `nil` when unavailable. It never searches the host PATH.

## Choose a version

An unversioned request selects the catalog's default for your platform when first
resolved. This is usually the newest packaged release; platforms an upstream has
dropped can have an older default. The package browser shows retained versions
and their platform support, so you can choose an exact release:

```sh
rb run jq@1.8.2 -- --version
rb use jq@1.8.2
```

The same syntax works in Lua: `rb.package("jq@1.8.2")`. Explicit versions never
fall back to another release. A retained version is available for selection;
it does not necessarily become the default.

Repeated runs and installs reuse saved resolutions. Use `--update` to refresh
unversioned requests; exact versions stay fixed. See
[updates and offline use](/guide/package-locks) for each workflow.

## Set up your shell

Add these lines to `~/.bashrc` for Bash or `~/.zshrc` for Zsh:

```sh
export PATH="$HOME/.rootbeer/bin:$PATH"
eval "$(rb env)"
```

The first line finds `rb` in its default installation directory. The second adds
commands from your user and configuration profiles. Fish syntax is not supported.

If you manage Zsh with Rootbeer, [`zsh.config()`](/modules/zsh) sets up package
commands automatically in login shells. Run `rb apply`, then `zsh -l` to load them.

## Platform support

Rootbeer installs prebuilt command-line tools on Apple silicon macOS and on Linux
ARM64 and x86-64. Intel macOS is unsupported. Filter the package browser for your
platform; support varies by version
and does not imply compatibility with every OS release or Linux distribution.
Installation does not compile packages locally.

For tools outside the catalog, see [other package sources](/guide/package-sources).
Catalog packages may also export macOS app bundles. For other desktop apps and
service integration, use [Homebrew](/modules/brew) or another system package manager.
