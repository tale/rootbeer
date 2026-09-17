# Updates and offline use

Rootbeer remembers resolved versions so repeated installs do not unexpectedly
change your tools. `rb run` and `rb use` save requests in Rootbeer's state directory;
Lua configurations save them in `rootbeer.lock` alongside `init.lua`.

## Update tools installed with run or use

Refresh a request before running it, or update selected installed packages:

```sh
rb run jq --update -- --version
rb use --update jq ripgrep
```

`rb use --update` updates the packages you name and keeps other installed tools.
`rb run --update` refreshes its process's packages without changing your installed
user profile. Both commands share cached request resolutions, so a later
`rb use jq` can use the version refreshed by `rb run jq --update`.

An exact request such as `jq@1.8.2` keeps that upstream version, including with
`--update`; its packaging or download details may change. These commands do not
read or modify your configuration's `rootbeer.lock`.

### Run or install offline

After resolving and downloading a request on this machine:

```sh
rb run jq --offline -- --version
rb use --offline jq
```

Offline use requires both the saved request and its installed or cached contents.
For `rb run -p`, every requested package must be cached. A different version or
request may need an online run first. `--offline` and `--update` cannot be combined.

## Update a Lua configuration

```sh
rb apply --update
```

This refreshes unpinned packages declared in your configuration and saves the
results in `rootbeer.lock`. Review and commit that file alongside `init.lua`.
Exact versions in your configuration stay fixed, though packaging details may change.

| Command              | Behavior                                                                                               |
| -------------------- | ------------------------------------------------------------------------------------------------------ |
| `rb apply`           | Use saved versions. Update the lock if your package configuration or platform changed.                 |
| `rb apply --update`  | Fetch current package information and refresh unpinned packages. Requires internet access.             |
| `rb apply --locked`  | Require the lock to match your configuration and platform. Missing package contents may be downloaded. |
| `rb apply --offline` | Require a matching lock and packages already installed or cached on this machine.                      |

If you [selected a custom index](/guide/package-sources#use-another-index),
updates keep using that snapshot until you change its URL and checksum.

`rb update` selects the latest published Rootbeer package from the signed index,
rather than the separate nightly download channel:

- Standalone installations replace only their `rb` executable, atomically.
- Installations made with `rb use rootbeer` switch the user profile to the updated
  package, preserving the original version or source selection and other packages.
- Lua-managed installations direct you to `rb apply --update`; they never change
  your configuration lock implicitly. Explicit version pins remain unchanged.

Run the command through the owning profile. Direct store paths and unrecognized
symlinks cannot self-update. Older user profiles without saved requests require
one explicit `rb use rootbeer --update` (or `rootbeer@VERSION`) to record your choice.

A matching configuration lock remains usable after upgrading `rb`.

## What offline mode covers

A saved lock alone is not enough: package contents must already be on the machine.
Rootbeer reports missing contents instead of downloading them. Cached contents can
remain usable even if the original download is no longer available.

Offline mode controls package resolution and downloads. A tool launched by
`rb run`, or a command in your Lua configuration, can still access the network.

## Using multiple machines

A configuration lockfile covers one platform. Applying on a different platform
rewrites it for that machine; `--locked` refuses this change.
[Profiles](/guide/profiles) can choose packages per machine, but cannot make one
lockfile cover multiple platforms.

Keep a checkout and lock on each machine when you need to preserve different
platforms' resolutions. Standalone `rb run` and `rb use` caches are local to each
machine; resolving there selects that platform's packages.
