# Updates and offline use

`rootbeer.lock` saves the package versions and downloads used by your configuration.
Commit it alongside `init.lua` so later installs use the same packages.

## Update packages

```sh
rb apply --update
```

This updates unpinned packages and saves the new versions in `rootbeer.lock`.
Review and commit the changes. Versions specified in `init.lua` stay fixed,
but their packaging or download details can change.

`rb update` updates Rootbeer itself. `rb apply --update` updates your packages.

| Command | Behavior |
| ------- | -------- |
| `rb apply` | Use saved versions. Update the lock if your package configuration or platform changed. |
| `rb apply --update` | Fetch current package information and update unpinned packages. Requires internet access. |
| `rb apply --locked` | Require the lock to match your package configuration and platform. Missing packages may be downloaded. |
| `rb apply --offline` | Require a matching lock and packages already downloaded on this machine. |

If you [selected a custom index](/guide/package-sources#use-another-index),
updates keep using it until you change that selection in your configuration.

## Install without internet access

```sh
rb apply --offline
```

The packages must already be installed or cached on this machine. A lockfile
alone is not enough. If a package is missing, Rootbeer reports an error instead
of downloading it. Cached packages remain usable even if the original download
is no longer available.

This flag controls package downloads; commands and other integrations in your
configuration may still need internet access.

## Using multiple machines

A lockfile currently covers one platform. Applying your configuration on a
different platform rewrites the lock for that machine; `--locked` refuses this
change. [Profiles](/guide/profiles) can choose different packages per machine,
but cannot make one lockfile cover multiple platforms.

If you need to preserve locks for different platforms, keep a checkout and its
lock on each machine, and avoid overwriting one platform's lock with another's.

Updating Rootbeer itself does not require replacing a matching lockfile.
