# Getting Started

::: tip
Check out the ["What is Rootbeer?"](/guide/what-is-rootbeer) overview for a
high-level introduction if you aren't already familiar with what Rootbeer does.
This guide assumes you already understand the basics and want to get started
immediately.
:::

## Installation

Install Rootbeer with a single command:

```sh
sh -c "$(curl -fsSL https://rootbeer.tale.me/rb.sh)"
```

> This will install the latest version of the `rb` CLI to `~/.rootbeer/bin`.

To start using Rootbeer, run:

```sh
# To start fresh
rb init

# To clone an existing repo by name (GitHub only)
rb init tale/dotfiles

rb init https://tangled.org/tale.me/dotfiles.git # Any git URL
rb init /path/to/local/repo # Local path
```

Running `rb init` will setup the `~/.config/rootbeer/` directory which contains
your source configuration. This is the directory you'll generally want to edit
and commit to git. Run `rb cd` or `rb edit` to jump into this directory.

### Private repos and SSH

When you run `rb init` with a repo argument, Rootbeer will clone via HTTPS by
default. If your repo is private and requires SSH, you can use the `--ssh` which
assumes that your SSH keys are already set up:

```sh
# Will clone git@github.com:tale/dotfiles.git
rb init --ssh tale/dotfiles
```

Alternatively, you may choose to clone with HTTPS and then switch to SSH later.
This is useful if you have your SSH keys set up during the initial `rb apply`.
Run `rb remote ssh` to switch the remote URL to SSH, or declare the remote in
your config with [`rb.remote()`](/reference/core#remote-url):

```lua
-- At the bottom of your init.lua, or anywhere in your config
rb.remote("git@github.com:tale/dotfiles.git")
```

## Creating Your Config

::: tip
Part of Rootbeer's power is that you can use the full expressiveness of Lua to
write your config. You can break up your config into multiple files using
`require()`, use loops and conditionals, define your own helper functions, and
more.
:::

When running an execution, Rootbeer will evaluate the `init.lua` file in your
source directory. This is where you'll define your config using the
[Rootbeer API](/reference/) and several provided [modules](/modules/). The API
gives you several building blocks and high-level abstractions to write your
config in a clear way.

### Config Profiles

Rootbeer provides several paradigms for organizing your config, but a common
pattern is to define profiles for different machines and then declare your
desired state within those profiles. They're generally versatile and can be used
in a variety of ways. Check out the [profiles guide](/guide/profiles) for more
details and examples.

### Applying Your Config

```sh
rb apply
```

Run `rb apply` to evaluate your config and write files into place. Rootbeer will
fully analyze your config for correctness and then apply all changes in a single
execution. If you want to see what would change without actually writing files,
use `rb apply --dry-run` or `rb apply -n` to preview the planned changes.

### Add Packages

Open [the package catalog](/packages/), select your platform, and copy declarations
for the tools you want into `init.lua`. Follow [the package guide](/guide/packages)
to source Rootbeer's package environment from your shell.

Run `rb apply`, then commit `rootbeer.lock` with your config. Normal applies keep
matching selections stable. Use `rb apply --update` when you want to refresh them,
or `rb apply --offline` when the lock and package contents are already cached.

### Editor Autocomplete

`rb init` will automatically set up a `.luarc.json` that enables you to have
full autocomplete and type checking for the Rootbeer API in any editor that has
been configured to use Lua's Language Server.

The generated `.luarc.json` points your editor at Rootbeer's Lua type definitions.
No custom LuaLS plugin is required.

## Updating Rootbeer

Rootbeer is still in early development, so expect to update frequently as new
features and improvements are made. To update to the latest version, run:

```sh
rb update
```

## Next Steps

You may want to brush up on some of the core concepts and features of Rootbeer.

- [Core Concepts](/guide/what-is-rootbeer#core-concepts): Quick overview.
- [Packages](/guide/packages): Managed packages and lockfile workflow.
- [Rootbeer API](/reference/): General purpose functions.
- [Modules](/modules/): High-level abstractions for popular tools and platforms.
