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

`--app` opens that exact bundle from the verified package store. It does not copy
it into Applications or configure login items. Arguments after `--` go to the app;
macOS may reuse an already running instance. Use `--bin` separately for command-line
tools. App permissions, such as Bobrwm's Accessibility access, remain managed by
macOS.

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

rb.package("jq")
rb.package("ripgrep")
```

```sh
rb apply
eval "$(rb env)"
rg --version
```

Commit `init.lua` and the resulting `rootbeer.lock` to keep the configuration and
resolved package versions together.

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
For desktop applications and services, use [Homebrew](/modules/brew) or another
system package manager.
